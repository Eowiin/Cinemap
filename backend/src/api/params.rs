use std::str::FromStr;

use chrono::{NaiveDate, NaiveTime};
use serde::Deserialize;
use sqlx::SqlitePool;

use super::error::{ApiResult, AppError};
use super::geo::Position;
use crate::text;
use crate::time::DATE_FORMAT;

const DEFAULT_RADIUS_KM: f64 = 15.0;
const MAX_RADIUS_KM: f64 = 100.0;
const DEFAULT_LIMIT: u32 = 50;
const MAX_LIMIT: u32 = 200;
const MIN_SEARCH_CHARS: usize = 2;

/// Tous les paramètres de query, en texte brut : la conversion est faite par
/// les fonctions ci-dessous, pour que les erreurs suivent le format du contrat
/// au lieu du 400 en texte d'axum. Un paramètre inutile à un endpoint est ignoré.
#[derive(Debug, Default, Deserialize)]
pub struct RawQuery {
    pub date: Option<String>,
    pub after: Option<String>,
    pub version: Option<String>,
    pub lat: Option<String>,
    pub lng: Option<String>,
    pub radius_km: Option<String>,
    pub limit: Option<String>,
    pub art_et_essai: Option<String>,
    pub q: Option<String>,
    pub cards: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VersionFilter {
    Vf,
    /// VO + VOST : « pas doublé en français ».
    Vo,
}

impl VersionFilter {
    pub fn as_sql(self) -> &'static str {
        match self {
            VersionFilter::Vf => "VF",
            VersionFilter::Vo => "VO",
        }
    }
}

pub fn parse_date(raw: Option<&str>, today: NaiveDate) -> ApiResult<NaiveDate> {
    match raw {
        None => Ok(today),
        Some(s) => NaiveDate::parse_from_str(s, DATE_FORMAT).map_err(|_| {
            AppError::BadRequest(format!("Date invalide : {s}. Format attendu : YYYY-MM-DD"))
        }),
    }
}

pub fn parse_after(raw: Option<&str>) -> ApiResult<Option<NaiveTime>> {
    raw.map(|s| {
        NaiveTime::parse_from_str(s, "%H:%M").map_err(|_| {
            AppError::BadRequest(format!("Heure invalide : {s}. Format attendu : HH:MM"))
        })
    })
    .transpose()
}

/// Borne à comparer au `starts_at` complet : la séance de 00h15 rattachée au
/// jour ciné `date` (son `starts_at` est le lendemain) reste après 22:00.
pub fn after_bound(date: NaiveDate, after: Option<NaiveTime>) -> Option<String> {
    after.map(|time| date.and_time(time).format("%Y-%m-%dT%H:%M:%S").to_string())
}

pub fn parse_version(raw: Option<&str>) -> ApiResult<Option<VersionFilter>> {
    match raw {
        None => Ok(None),
        Some("VF") => Ok(Some(VersionFilter::Vf)),
        Some("VO") => Ok(Some(VersionFilter::Vo)),
        Some(s) => Err(AppError::BadRequest(format!(
            "Version invalide : {s}. Valeurs possibles : VF, VO"
        ))),
    }
}

pub fn parse_bool(raw: Option<&str>, name: &str) -> ApiResult<Option<bool>> {
    match raw {
        None => Ok(None),
        Some("true") => Ok(Some(true)),
        Some("false") => Ok(Some(false)),
        Some(s) => Err(AppError::BadRequest(format!(
            "{name} invalide : {s}. Valeurs possibles : true, false"
        ))),
    }
}

fn parse_value<T: FromStr>(s: &str, name: &str) -> ApiResult<T> {
    s.parse()
        .map_err(|_| AppError::BadRequest(format!("{name} invalide : {s}")))
}

fn parse_number<T: FromStr>(raw: Option<&str>, name: &str) -> ApiResult<Option<T>> {
    raw.map(|s| parse_value(s, name)).transpose()
}

pub fn parse_movie_id(raw: &str) -> ApiResult<i64> {
    parse_value(raw, "Identifiant de film")
}

pub fn parse_position(lat: Option<&str>, lng: Option<&str>) -> ApiResult<Option<Position>> {
    let (lat, lng) = match (lat, lng) {
        (None, None) => return Ok(None),
        (Some(lat), Some(lng)) => (lat, lng),
        _ => return Err(AppError::BadRequest("lat et lng vont ensemble".into())),
    };
    let lat: f64 = parse_value(lat, "lat")?;
    let lng: f64 = parse_value(lng, "lng")?;
    // "NaN".parse::<f64>() réussit : les comparaisons ci-dessous seraient fausses
    // sans être des erreurs, d'où is_finite().
    if !lat.is_finite() || !(-90.0..=90.0).contains(&lat) {
        return Err(AppError::BadRequest(format!("lat hors limites : {lat}")));
    }
    if !lng.is_finite() || !(-180.0..=180.0).contains(&lng) {
        return Err(AppError::BadRequest(format!("lng hors limites : {lng}")));
    }
    Ok(Some(Position { lat, lng }))
}

pub fn parse_radius(raw: Option<&str>) -> ApiResult<f64> {
    let radius = parse_number(raw, "radius_km")?.unwrap_or(DEFAULT_RADIUS_KM);
    if radius > 0.0 && radius <= MAX_RADIUS_KM {
        Ok(radius)
    } else {
        Err(AppError::BadRequest(format!(
            "radius_km doit être compris entre 0 (exclu) et {MAX_RADIUS_KM}"
        )))
    }
}

pub fn parse_limit(raw: Option<&str>) -> ApiResult<u32> {
    let limit = parse_number(raw, "limit")?.unwrap_or(DEFAULT_LIMIT);
    if (1..=MAX_LIMIT).contains(&limit) {
        Ok(limit)
    } else {
        Err(AppError::BadRequest(format!(
            "limit doit être compris entre 1 et {MAX_LIMIT}"
        )))
    }
}

/// Paramètre `cards` (ids séparés par des virgules), vérifié contre la table `cards` :
/// la liste en JSON, prête pour `json_each` en SQL, ou `None` sans filtre.
pub async fn parse_cards(raw: Option<&str>, pool: &SqlitePool) -> ApiResult<Option<String>> {
    let Some(raw) = raw else {
        return Ok(None);
    };
    let ids: Vec<&str> = raw
        .split(',')
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .collect();
    if ids.is_empty() {
        return Err(AppError::BadRequest("cards ne peut pas être vide".into()));
    }
    let known = sqlx::query_scalar!(r#"SELECT id AS "id!: String" FROM cards"#)
        .fetch_all(pool)
        .await?;
    if let Some(unknown) = ids.iter().find(|id| !known.iter().any(|k| k == *id)) {
        return Err(AppError::BadRequest(format!(
            "Carte inconnue : {unknown}. Valeurs possibles : {}",
            known.join(", ")
        )));
    }
    Ok(Some(serde_json::to_string(&ids)?))
}

/// Texte de recherche normalisé (minuscules, sans accents), au moins 2 caractères.
pub fn parse_search_query(raw: Option<&str>) -> ApiResult<String> {
    let q = text::normalize(raw.unwrap_or_default().trim());
    // chars() et pas len() : len() compte des octets.
    if q.chars().count() < MIN_SEARCH_CHARS {
        return Err(AppError::BadRequest(format!(
            "La recherche doit faire au moins {MIN_SEARCH_CHARS} caractères"
        )));
    }
    Ok(q)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn today() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 10, 8).unwrap()
    }

    fn is_bad_request<T: std::fmt::Debug>(result: ApiResult<T>) -> bool {
        matches!(result, Err(AppError::BadRequest(_)))
    }

    #[test]
    fn date_defaults_to_today() {
        assert_eq!(parse_date(None, today()).unwrap(), today());
    }

    #[test]
    fn date_parses_iso_format() {
        assert_eq!(
            parse_date(Some("2026-10-09"), today()).unwrap(),
            NaiveDate::from_ymd_opt(2026, 10, 9).unwrap()
        );
    }

    #[test]
    fn date_rejects_invalid_values() {
        for raw in ["2026-13-01", "08/10/2026", ""] {
            assert!(is_bad_request(parse_date(Some(raw), today())), "{raw}");
        }
    }

    #[test]
    fn after_accepts_with_and_without_leading_zero() {
        assert_eq!(
            parse_after(Some("20:00")).unwrap(),
            NaiveTime::from_hms_opt(20, 0, 0)
        );
        assert_eq!(
            parse_after(Some("9:05")).unwrap(),
            NaiveTime::from_hms_opt(9, 5, 0)
        );
        assert_eq!(parse_after(None).unwrap(), None);
    }

    #[test]
    fn after_rejects_invalid_values() {
        for raw in ["25:00", "20h", ""] {
            assert!(is_bad_request(parse_after(Some(raw))), "{raw}");
        }
    }

    #[test]
    fn after_bound_is_a_full_starts_at() {
        assert_eq!(
            after_bound(today(), NaiveTime::from_hms_opt(20, 0, 0)).as_deref(),
            Some("2026-10-08T20:00:00")
        );
        assert_eq!(after_bound(today(), None), None);
    }

    #[test]
    fn version_accepts_only_uppercase_vf_and_vo() {
        assert_eq!(parse_version(None).unwrap(), None);
        assert_eq!(parse_version(Some("VF")).unwrap(), Some(VersionFilter::Vf));
        assert_eq!(parse_version(Some("VO")).unwrap(), Some(VersionFilter::Vo));
        for raw in ["VOST", "vf", "xx"] {
            assert!(is_bad_request(parse_version(Some(raw))), "{raw}");
        }
    }

    #[test]
    fn bool_accepts_only_true_and_false() {
        assert_eq!(parse_bool(Some("true"), "x").unwrap(), Some(true));
        assert_eq!(parse_bool(Some("false"), "x").unwrap(), Some(false));
        assert_eq!(parse_bool(None, "x").unwrap(), None);
        assert!(is_bad_request(parse_bool(Some("oui"), "x")));
        assert!(is_bad_request(parse_bool(Some("1"), "x")));
    }

    #[test]
    fn movie_id_must_be_numeric() {
        assert_eq!(parse_movie_id("1000032855").unwrap(), 1000032855);
        assert!(is_bad_request(parse_movie_id("abc")));
    }

    #[test]
    fn position_needs_both_coordinates() {
        assert_eq!(parse_position(None, None).unwrap(), None);
        assert_eq!(
            parse_position(Some("48.8566"), Some("2.3522")).unwrap(),
            Some(Position {
                lat: 48.8566,
                lng: 2.3522
            })
        );
        assert!(is_bad_request(parse_position(Some("48"), None)));
        assert!(is_bad_request(parse_position(None, Some("2"))));
    }

    #[test]
    fn position_rejects_out_of_range_and_nan() {
        for (lat, lng) in [
            ("91", "2"),
            ("48", "-181"),
            ("NaN", "2"),
            ("48", "inf"),
            ("abc", "2"),
        ] {
            assert!(
                is_bad_request(parse_position(Some(lat), Some(lng))),
                "{lat} {lng}"
            );
        }
    }

    #[test]
    fn radius_defaults_to_15_and_is_bounded() {
        assert_eq!(parse_radius(None).unwrap(), 15.0);
        assert_eq!(parse_radius(Some("100")).unwrap(), 100.0);
        for raw in ["0", "-1", "101", "NaN", "abc"] {
            assert!(is_bad_request(parse_radius(Some(raw))), "{raw}");
        }
    }

    #[test]
    fn limit_defaults_to_50_and_is_bounded() {
        assert_eq!(parse_limit(None).unwrap(), 50);
        assert_eq!(parse_limit(Some("1")).unwrap(), 1);
        assert_eq!(parse_limit(Some("200")).unwrap(), 200);
        for raw in ["0", "201", "-1", "abc"] {
            assert!(is_bad_request(parse_limit(Some(raw))), "{raw}");
        }
    }

    #[test]
    fn search_query_is_normalized_and_long_enough() {
        assert_eq!(
            parse_search_query(Some("  Ciné Cité ")).unwrap(),
            "cine cite"
        );
        assert_eq!(parse_search_query(Some("ÉT")).unwrap(), "et");
        for raw in [None, Some(""), Some(" a "), Some("é"), Some("%")] {
            assert!(is_bad_request(parse_search_query(raw)), "{raw:?}");
        }
    }
}
