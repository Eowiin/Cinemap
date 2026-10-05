use std::collections::{HashMap, HashSet};

use anyhow::{Context, ensure};
use csv::Reader;
use reqwest::multipart::{Form, Part};
use serde::Deserialize;
use sqlx::SqlitePool;
use tracing::{info, warn};

use crate::text::{department_from_insee, get_postal_code, normalize};

const GEOCODE_URL: &str = "https://data.geopf.fr/geocodage/search/csv";

#[derive(Debug, Deserialize)]
struct GeoData {
    id: String,
    latitude: Option<f64>,
    longitude: Option<f64>,
    result_score: Option<f64>,
    result_postcode: String,
    result_city: String,
    result_citycode: String,
    result_status: String,
    result_type: String,
    #[serde(default)]
    result_oldcity: String,
    #[serde(default)]
    result_name: String,
    #[serde(default)]
    result_context: String,
}

#[derive(Debug)]
struct PreparedAddress {
    full_query: String,
    postcode: Option<String>,
    municipality_query: Option<String>,
    city: Option<String>,
    arrondissement: Option<String>,
}

struct CinemaToGeocode {
    id: String,
    name: String,
    address: Option<String>,
}

struct GeoRequest {
    id: String,
    address: String,
    postcode: String,
    result_type: String,
}

/// Geocode stored addresses; no AlloCiné request is made here.
pub async fn geocode_cinemas(
    pool: &SqlitePool,
    client: &reqwest::Client,
    all: bool,
) -> anyhow::Result<()> {
    geocode_cinemas_at(pool, client, all, GEOCODE_URL).await
}

async fn geocode_cinemas_at(
    pool: &SqlitePool,
    client: &reqwest::Client,
    all: bool,
    url: &str,
) -> anyhow::Result<()> {
    let cinemas = load_cinemas_to_geocode(pool, all).await?;
    if cinemas.is_empty() {
        info!("Aucun géocodage nécessaire");
        return Ok(());
    }
    let prepared: Vec<_> = cinemas
        .iter()
        .map(|c| prepare_address(c.address.as_deref().unwrap_or_default()))
        .collect();
    let requests: Vec<_> = cinemas
        .iter()
        .zip(&prepared)
        .filter(|(_, a)| !a.full_query.is_empty())
        .map(|(c, a)| GeoRequest {
            id: c.id.clone(),
            address: a.full_query.clone(),
            postcode: a.postcode.clone().unwrap_or_default(),
            result_type: String::new(),
        })
        .collect();
    info!(
        cinemas = cinemas.len(),
        requetes = requests.len(),
        "Adresses à géocoder"
    );
    let mut primary = geocode_batch(client, &requests, url).await?;
    let fallback_requests: Vec<_> = cinemas
        .iter()
        .zip(&prepared)
        .filter(|(c, a)| {
            primary
                .get(&c.id)
                .is_none_or(|r| validate_result(a, r, false).is_err())
        })
        .flat_map(|(c, a)| {
            // Retry the complete address without its postal filter (CEDEX), and
            // explicitly request a municipality so a street cannot outrank it.
            let mut requests = vec![GeoRequest {
                id: format!("{}:address", c.id),
                address: a.full_query.clone(),
                postcode: String::new(),
                result_type: String::new(),
            }];
            if let Some(address) = &a.municipality_query {
                requests.push(GeoRequest {
                    id: format!("{}:municipality", c.id),
                    address: address.clone(),
                    postcode: String::new(),
                    result_type: "municipality".into(),
                });
            }
            requests.into_iter().filter(|r| !r.address.is_empty())
        })
        .collect();
    info!(requetes = fallback_requests.len(), "Repli sur la commune");
    let mut fallback = geocode_batch(client, &fallback_requests, url).await?;

    // Finish both HTTP requests and validate every response before opening a transaction.
    let mut accepted = Vec::with_capacity(cinemas.len());
    let mut precise_count = 0;
    let mut municipality_count = 0;
    for (cinema, address) in cinemas.iter().zip(&prepared) {
        let first = primary.remove(&cinema.id);
        let retry = fallback.remove(&format!("{}:address", cinema.id));
        let municipality = fallback.remove(&format!("{}:municipality", cinema.id));
        let decision = if first
            .as_ref()
            .is_some_and(|r| validate_result(address, r, false).is_ok())
        {
            first
        } else if retry
            .as_ref()
            .is_some_and(|r| validate_result(address, r, true).is_ok())
        {
            retry
        } else if municipality
            .as_ref()
            .is_some_and(|r| validate_result(address, r, true).is_ok())
        {
            municipality
        } else {
            let reason = municipality
                .as_ref()
                .or(retry.as_ref())
                .or(first.as_ref())
                .and_then(|r| validate_result(address, r, true).err())
                .unwrap_or("adresse absente ou inexploitable");
            warn!(id = %cinema.id, name = %cinema.name, address = ?cinema.address,
                reason, "Aucune position cohérente ; anciennes coordonnées invalidées");
            None
        };
        if let Some(data) = &decision {
            if matches!(data.result_type.as_str(), "housenumber" | "street") {
                precise_count += 1;
            } else {
                municipality_count += 1;
            }
        }
        accepted.push(decision);
    }
    save_geodata(pool, &cinemas, &prepared, &accepted).await?;
    info!(
        examines = cinemas.len(),
        precis = precise_count,
        communes_ou_lieux_dits = municipality_count,
        non_resolus = cinemas.len() - precise_count - municipality_count,
        "Géocodage terminé"
    );
    Ok(())
}

async fn load_cinemas_to_geocode(
    pool: &SqlitePool,
    all: bool,
) -> anyhow::Result<Vec<CinemaToGeocode>> {
    Ok(sqlx::query_as!(
        CinemaToGeocode,
        r#"SELECT id AS "id!", name, address FROM cinemas
           WHERE id IS NOT NULL AND (? OR lat IS NULL OR lng IS NULL) ORDER BY id"#,
        all
    )
    .fetch_all(pool)
    .await?)
}

fn build_geocoding_form(requests: &[GeoRequest]) -> anyhow::Result<Form> {
    let mut writer = csv::Writer::from_writer(Vec::new());
    writer.write_record(["id", "adresse", "postcode", "type"])?;
    for request in requests {
        writer.write_record([
            &request.id,
            &request.address,
            &request.postcode,
            &request.result_type,
        ])?;
    }
    let part = Part::bytes(writer.into_inner()?)
        .file_name("cinemas.csv")
        .mime_str("text/csv")?;
    Ok(Form::new()
        .part("data", part)
        .text("columns", "adresse")
        .text("postcode", "postcode")
        .text("type", "type"))
}

async fn geocode_batch(
    client: &reqwest::Client,
    requests: &[GeoRequest],
    url: &str,
) -> anyhow::Result<HashMap<String, GeoData>> {
    if requests.is_empty() {
        return Ok(HashMap::new());
    }
    let raw_csv = client
        .post(url)
        .multipart(build_geocoding_form(requests)?)
        .send()
        .await?
        .error_for_status()?
        .text()
        .await?;
    let results = get_geodata(&raw_csv)?;
    // A truncated or inconsistent batch is a protocol error, not a geocoding failure.
    let expected: HashSet<_> = requests.iter().map(|r| r.id.as_str()).collect();
    let mut by_id = HashMap::new();
    for result in results {
        ensure!(
            expected.contains(result.id.as_str()),
            "Identifiant inattendu dans le CSV : {}",
            result.id
        );
        let id = result.id.clone();
        ensure!(
            by_id.insert(id.clone(), result).is_none(),
            "Identifiant répété dans le CSV : {id}"
        );
    }
    ensure!(
        by_id.len() == expected.len(),
        "Réponse de géocodage incomplète"
    );
    Ok(by_id)
}

fn get_geodata(raw_csv: &str) -> anyhow::Result<Vec<GeoData>> {
    Reader::from_reader(raw_csv.as_bytes())
        .deserialize::<GeoData>()
        .collect::<Result<Vec<_>, _>>()
        .context("Réponse CSV de géocodage invalide")
}

/// Normalize only a terminal postal-code/city suffix, never arbitrary street numbers.
fn normalize_postal_spacing(address: &str) -> String {
    let mut result = address.trim().to_owned();
    let bytes = result.as_bytes();
    let candidate = bytes.windows(6).enumerate().rev().find_map(|(i, w)| {
        let valid = w[..2].iter().all(u8::is_ascii_digit)
            && w[2] == b' '
            && w[3..].iter().all(u8::is_ascii_digit)
            && (i == 0 || bytes[i - 1].is_ascii_whitespace())
            && bytes.get(i + 6).is_some_and(u8::is_ascii_whitespace);
        if !valid {
            return None;
        }
        let suffix = result[i + 6..].trim();
        // Do not reinterpret "12 345 Rue ... 75001 Paris" as a postal code.
        let city_like = !suffix.is_empty()
            && !suffix.contains(['\n', '\r'])
            && !suffix.chars().any(|c| c.is_ascii_digit())
            && !matches!(
                normalize(suffix).split_whitespace().next(),
                Some("rue" | "avenue" | "boulevard" | "route" | "chemin" | "place")
            );
        city_like.then_some(i + 2)
    });
    if let Some(space) = candidate {
        result.remove(space);
    }
    result
}

fn prepare_address(address: &str) -> PreparedAddress {
    let address = normalize_postal_spacing(address);
    let first_line = address
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or_default();
    let position = get_postal_code(&address);
    let postcode = position.map(|i| address[i..i + 5].to_owned());
    let city = position
        .map(|i| {
            address[i + 5..]
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
        })
        .filter(|city| !city.is_empty());
    let arrondissement = postcode
        .as_deref()
        .zip(city.as_deref())
        .and_then(|(cp, city)| arrondissement_from_address(cp, city));
    let municipality_query = postcode.as_ref().zip(city.as_ref()).map(|(cp, city)| {
        if let Some((name, number)) = arrondissement_details(cp, city) {
            format!(
                "{name} {number}{} Arrondissement",
                if number == 1 { "er" } else { "e" }
            )
        } else {
            format!("{cp} {city}")
        }
    });
    let full_query = if position.is_some_and(|i| i < first_line.len()) {
        first_line.to_owned()
    } else if let Some(i) = position {
        format!("{} {}", first_line, &address[i..])
    } else {
        address.clone()
    };
    PreparedAddress {
        full_query: full_query.split_whitespace().collect::<Vec<_>>().join(" "),
        postcode,
        municipality_query,
        city,
        arrondissement,
    }
}

fn city_key(city: &str) -> String {
    normalize(city)
        .split(|c: char| !c.is_alphanumeric())
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

fn arrondissement_details(postcode: &str, city: &str) -> Option<(&'static str, u16)> {
    let key = city_key(city);
    let city_name = key.split_whitespace().next()?;
    let number = postcode.get(2..)?.parse::<u16>().ok()?;
    match (postcode.get(..2)?, city_name, number) {
        ("75", "paris", 1..=20) => Some(("Paris", number)),
        ("69", "lyon", 1..=9) => Some(("Lyon", number)),
        ("13", "marseille", 1..=16) => Some(("Marseille", number)),
        _ => None,
    }
}

fn arrondissement_from_address(postcode: &str, city: &str) -> Option<String> {
    let (name, number) = arrondissement_details(postcode, city)?;
    Some(match name {
        "Paris" => format!("751{number:02}"),
        "Lyon" => format!("693{:02}", 80 + number),
        "Marseille" => format!("132{number:02}"),
        _ => unreachable!(),
    })
}

fn department_matches(postcode: &str, insee: &str) -> bool {
    if postcode.len() != 5 || !postcode.bytes().all(|b| b.is_ascii_digit()) {
        return false;
    }
    let Some(department) = department_from_insee(insee) else {
        return false;
    };
    if postcode.starts_with("20") {
        matches!(department, "2A" | "2B")
    } else if postcode.starts_with("97") {
        department == &postcode[..3]
    } else {
        department == &postcode[..2]
    }
}

fn validate_result(
    address: &PreparedAddress,
    data: &GeoData,
    fallback: bool,
) -> Result<(), &'static str> {
    if data.result_status != "ok" {
        return Err("aucun résultat de l’API");
    }
    if !data
        .latitude
        .is_some_and(|v| v.is_finite() && (-90.0..=90.0).contains(&v))
        || !data
            .longitude
            .is_some_and(|v| v.is_finite() && (-180.0..=180.0).contains(&v))
    {
        return Err("coordonnées absentes ou invalides");
    }
    let Some(postcode) = address.postcode.as_deref() else {
        return Err("code postal source introuvable");
    };
    let code = &data.result_citycode;
    if code.len() != 5
        || !(code.bytes().all(|b| b.is_ascii_digit())
            || ((code.starts_with("2A") || code.starts_with("2B"))
                && code[2..].bytes().all(|b| b.is_ascii_digit())))
    {
        return Err("code INSEE invalide");
    }
    if !department_matches(postcode, code) {
        return Err("département incohérent");
    }
    if address
        .arrondissement
        .as_ref()
        .is_some_and(|expected| expected != code)
    {
        return Err("arrondissement incohérent");
    }
    // result_context names the department and region, not the requested place.
    // Use its department only as a consistency check, never as a name alias.
    if !data.result_context.is_empty()
        && data.result_context.split(',').next().map(str::trim) != department_from_insee(code)
    {
        return Err("contexte et code INSEE incohérents");
    }
    if !fallback && data.result_postcode != postcode {
        return Err("code postal différent de l’adresse");
    }
    if !matches!(
        data.result_type.as_str(),
        "housenumber" | "street" | "locality" | "municipality"
    ) {
        return Err("type de résultat non accepté");
    }
    // A street name can contain a different town's name: never use it as an alias.
    // For a locality, the name itself may identify the requested hamlet.
    if (fallback || matches!(data.result_type.as_str(), "locality" | "municipality"))
        && address.arrondissement.is_none()
        && !place_matches(address, data)
    {
        return Err("lieu différent de l’adresse");
    }
    Ok(())
}

fn place_matches(address: &PreparedAddress, data: &GeoData) -> bool {
    let Some(city) = address.city.as_deref() else {
        return false;
    };
    let expected = city_key(city);
    if expected.is_empty() {
        return false;
    }
    let current = city_key(&data.result_city);
    if current == expected || city_key(&data.result_oldcity) == expected {
        return true;
    }
    if data.result_type == "locality" {
        // Whole named component: "Porticcio" in "Les Marines 1, Porticcio".
        // Never accept "Petite Rivière Salée" or "route de Mulhouse" as aliases.
        if data
            .result_name
            .split([',', '(', ')'])
            .any(|part| city_key(part) == expected)
        {
            return true;
        }
    }
    // Shortened commune names and named districts, with the SAME postcode:
    // Les Adrets -> Les Adrets-de-l'Estérel; Cannes La Bocca -> Cannes.
    // Prefixes only: La Plagne must not match Aime-la-Plagne.
    matches!(data.result_type.as_str(), "locality" | "municipality")
        && address.postcode.as_deref() == Some(data.result_postcode.as_str())
        && !current.is_empty()
        && (current.starts_with(&format!("{expected} "))
            || expected.starts_with(&format!("{current} ")))
}

async fn save_geodata(
    pool: &SqlitePool,
    cinemas: &[CinemaToGeocode],
    prepared: &[PreparedAddress],
    accepted: &[Option<GeoData>],
) -> anyhow::Result<()> {
    let mut tx = pool.begin().await?;
    for ((cinema, address), data) in cinemas.iter().zip(prepared).zip(accepted) {
        let data = data.as_ref();
        let city_search = data.map(|d| normalize(&d.result_city));
        // Guard against an address changed by another importer during the HTTP calls.
        let latitude = data.and_then(|d| d.latitude);
        let longitude = data.and_then(|d| d.longitude);
        let score = data.and_then(|d| d.result_score);
        let city = data.map(|d| d.result_city.as_str());
        let insee = data.map(|d| d.result_citycode.as_str());
        let department = insee.and_then(department_from_insee);
        let geocode_type = data.map(|d| d.result_type.as_str());
        let update = sqlx::query!(
            "UPDATE cinemas SET lat = ?, lng = ?, geocode_score = ?, postal_code = ?,
                city = ?, city_search = ?, insee_code = ?, department = ?, geocode_type = ?
             WHERE id = ? AND address IS ?",
            latitude,
            longitude,
            score,
            address.postcode,
            city,
            city_search,
            insee,
            department,
            geocode_type,
            cinema.id,
            cinema.address
        )
        .execute(&mut *tx)
        .await?;
        ensure!(
            update.rows_affected() == 1,
            "Adresse modifiée pendant le géocodage de {} ; transaction annulée",
            cinema.id
        );
    }
    tx.commit().await?;
    Ok(())
}

#[cfg(test)]
mod tests;
