use std::{
    collections::{HashMap, HashSet},
    io::Cursor,
    time::Duration,
};

use anyhow::{Context, anyhow};
use calamine::{RangeDeserializerBuilder, Reader, Xlsx, open_workbook_from_rs};
use reqwest::Client;
use serde::Deserialize;
use sqlx::SqlitePool;
use strsim::jaro_winkler;
use tracing::{debug, info};

use crate::{
    client::{RETRY_DELAYS, fetch_with_retries},
    text::normalize,
};

const CNC_URL: &str =
    "https://www.data.gouv.fr/api/1/datasets/r/cdb918e7-7f1a-44fc-bf6f-c59d1614ed6d";

/// Le délai global du client (30 s) couvre toute la réponse, corps compris : trop court
/// pour ~2,5 Mo sur une connexion lente. Ce délai ne s'applique qu'à ce téléchargement.
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(120);

/// Similarité minimale entre deux noms de la même commune pour les associer.
const MIN_SIMILARITY: f64 = 0.85;

#[derive(Debug)]
pub struct CncCinema {
    pub cnc_id: i64,
    pub name: String,
    pub insee_code: String,
    pub screens: i64,
    pub seats: Option<i64>,
    pub art_et_essai: bool,
}

#[derive(Deserialize)]
struct CncRow {
    #[serde(rename = "NAutoC")]
    nautoc: f64,
    #[serde(rename = "NomEtab")]
    name: String,
    #[serde(rename = "Ecrans")]
    screens: f64,
    #[serde(rename = "fauteuils")]
    seats: Option<f64>,
    #[serde(rename = "DEPCOM")]
    depcom: String,
    #[serde(rename = "AE")]
    ae: String,
}

fn latest_year_sheet(sheet_names: &[String]) -> anyhow::Result<String> {
    sheet_names
        .iter()
        .filter_map(|name| name.trim().parse::<u16>().ok().map(|year| (year, name)))
        .max_by_key(|(year, _)| *year)
        .map(|(_, name)| name.clone())
        .context("Aucune feuille d'année dans le fichier CNC")
}

pub fn parse_cnc_xlsx(bytes: &[u8]) -> anyhow::Result<Vec<CncCinema>> {
    let cursor = Cursor::new(bytes);
    let mut workbook: Xlsx<_> = open_workbook_from_rs(cursor)?;
    let sheet = latest_year_sheet(&workbook.sheet_names())?;
    let default_range = workbook.worksheet_range(&sheet)?;
    let end = default_range.end().context("La feuille CNC est vide")?;
    let range = default_range.range((4, 0), end);
    let mut cnc_data: Vec<CncCinema> = Vec::with_capacity(range.height());
    let iter = RangeDeserializerBuilder::with_deserialize_headers::<CncRow>().from_range(&range)?;

    for result in iter {
        let row: CncRow = result?;

        cnc_data.push(CncCinema {
            cnc_id: checked_integer(row.nautoc)?,
            name: row.name.trim().to_owned(),
            insee_code: row.depcom.trim().to_owned(),
            screens: checked_integer(row.screens)?,
            seats: row.seats.map(checked_integer).transpose()?,
            art_et_essai: match row.ae.trim() {
                "OUI" => true,
                "NON" => false,
                other => {
                    return Err(anyhow!(
                        "Valeur « art et essai » (AE) invalide, erreur : {other}"
                    ));
                }
            },
        });
    }
    Ok(cnc_data)
}

/// Cinéma en base, déjà géocodé (le code INSEE vient du géocodage).
#[derive(Debug)]
pub struct LocatedCinema {
    pub id: String,
    pub name_search: String,
    pub insee_code: String,
}

/// Association d'un cinéma AlloCiné avec un établissement CNC.
#[derive(Debug)]
pub struct CncMatch<'a> {
    pub cinema_id: &'a str,
    pub cnc: &'a CncCinema,
    pub score: f64,
}

/// Télécharge le fichier CNC, l'associe aux cinémas géocodés et enregistre le résultat.
pub async fn enrich_cinemas(pool: &SqlitePool, client: &Client) -> anyhow::Result<()> {
    let bytes = fetch_with_retries(CNC_URL, &RETRY_DELAYS, || {
        client.get(CNC_URL).timeout(DOWNLOAD_TIMEOUT)
    })
    .await
    .context("Téléchargement du fichier CNC")?;
    let cnc = parse_cnc_xlsx(&bytes)?;
    // Le XLSX brut n'est plus utile : on libère ses ~2,5 Mo avant la suite.
    drop(bytes);

    let cinemas = sqlx::query_as!(
        LocatedCinema,
        r#"SELECT id AS "id!", name_search, insee_code AS "insee_code!"
           FROM cinemas WHERE insee_code IS NOT NULL"#
    )
    .fetch_all(pool)
    .await?;

    let matches = match_cinemas(&cinemas, &cnc);
    save_matches(pool, &matches).await?;

    info!(
        cinemas = cinemas.len(),
        cnc = cnc.len(),
        croises = matches.len(),
        "Enrichissement CNC enregistré"
    );
    Ok(())
}

/// Associe chaque cinéma à au plus un établissement CNC de la même commune, et
/// inversement. Les paires sont traitées de la plus ressemblante à la moins
/// ressemblante : si deux cinémas visent le même établissement, le plus proche gagne.
pub fn match_cinemas<'a>(cinemas: &'a [LocatedCinema], cnc: &'a [CncCinema]) -> Vec<CncMatch<'a>> {
    // Noms normalisés calculés une seule fois, regroupés par commune.
    let mut by_commune: HashMap<&str, Vec<(&CncCinema, String)>> = HashMap::new();
    for establishment in cnc {
        by_commune
            .entry(commune_code(&establishment.insee_code))
            .or_default()
            .push((establishment, comparable_name(&establishment.name)));
    }

    // Chaque paire garde aussi son `jaro_winkler` brut pour départager les égalités.
    let mut pairs: Vec<(CncMatch, f64)> = Vec::new();
    for cinema in cinemas {
        let Some(candidates) = by_commune.get(commune_code(&cinema.insee_code)) else {
            continue;
        };
        let name = comparable_name(&cinema.name_search);
        for (establishment, cnc_name) in candidates {
            let score = similarity(&name, cnc_name);
            debug!(
                cinema = %cinema.id,
                allocine = %name,
                cnc = %cnc_name,
                score,
                "Paire candidate"
            );
            if score >= MIN_SIMILARITY {
                let pair = CncMatch {
                    cinema_id: &cinema.id,
                    cnc: establishment,
                    score,
                };
                pairs.push((pair, jaro_winkler(&name, cnc_name)));
            }
        }
    }

    // Meilleur score d'abord ; à égalité (inclusion complète des deux côtés, ex.
    // « ugc cite » et « ugc cite part dieu » dans « ugc cite lyon part dieu »), le nom le
    // plus proche lettre à lettre gagne. Tri stable : résultat reproductible.
    pairs.sort_by(|(a, a_closeness), (b, b_closeness)| {
        b.score
            .total_cmp(&a.score)
            .then(b_closeness.total_cmp(a_closeness))
    });
    let mut used_cinemas = HashSet::new();
    let mut used_cnc = HashSet::new();
    pairs
        .into_iter()
        .map(|(pair, _)| pair)
        .filter(|pair| {
            let free =
                !used_cinemas.contains(pair.cinema_id) && !used_cnc.contains(&pair.cnc.cnc_id);
            if free {
                used_cinemas.insert(pair.cinema_id);
                used_cnc.insert(pair.cnc.cnc_id);
            }
            free
        })
        .collect()
}

/// Code INSEE de la commune entière pour Lyon et Marseille : le CNC les note
/// `69123` / `13055`, alors que le géocodage renvoie l'arrondissement (`69381`, `13201`…).
/// Paris est noté par arrondissement des deux côtés (`75101`…), on le garde tel quel.
fn commune_code(insee: &str) -> &str {
    if insee.starts_with("6938") {
        "69123"
    } else if insee.starts_with("132") {
        "13055"
    } else {
        insee
    }
}

/// Nom normalisé sans ponctuation ni mots génériques (« cinéma », « le »…), qui
/// diffèrent souvent entre AlloCiné et le CNC sans changer l'établissement.
/// Si le nom ne contient que des mots génériques (« Cinéma »), on les garde.
fn comparable_name(name: &str) -> String {
    const GENERIC: &[&str] = &[
        "cinema", "cinemas", "cine", "salle", "mega", "le", "la", "les", "l", "de", "du", "des",
        "d",
    ];
    let normalized = normalize(name);
    let words: Vec<&str> = normalized
        .split(|c: char| !c.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .map(|word| match word {
            "st" => "saint",
            "ste" => "sainte",
            other => other,
        })
        .collect();
    let specific: Vec<&str> = words
        .iter()
        .copied()
        .filter(|word| !GENERIC.contains(word))
        .collect();
    if specific.is_empty() {
        words.join(" ")
    } else {
        specific.join(" ")
    }
}

/// Le meilleur de deux mesures : `jaro_winkler` tolère les fautes et abréviations
/// (« st paul » / « saint paul ») ; l'inclusion des mots couvre les noms complétés
/// d'un côté (« comoedia » / « sete comoedia », « champo » / « champo espace jacques tati »).
fn similarity(a: &str, b: &str) -> f64 {
    jaro_winkler(a, b).max(word_containment(a, b))
}

/// Part des mots du nom le plus court présents dans l'autre nom.
fn word_containment(a: &str, b: &str) -> f64 {
    let (short, long) = if a.len() <= b.len() { (a, b) } else { (b, a) };
    let long_words: Vec<&str> = long.split(' ').collect();
    let short_words: Vec<&str> = short.split(' ').filter(|w| !w.is_empty()).collect();
    if short_words.is_empty() {
        return 0.0;
    }
    let found = short_words
        .iter()
        .filter(|w| long_words.contains(w))
        .count();
    found as f64 / short_words.len() as f64
}

/// Remplace toutes les données CNC en une transaction : un cinéma qui n'est plus
/// associé perd ses anciennes valeurs.
async fn save_matches(pool: &SqlitePool, matches: &[CncMatch<'_>]) -> anyhow::Result<()> {
    let mut tx = pool.begin().await?;
    sqlx::query!(
        "UPDATE cinemas SET cnc_id = NULL, screens = NULL, seats = NULL, art_et_essai = 0"
    )
    .execute(&mut *tx)
    .await?;

    for m in matches {
        sqlx::query!(
            "UPDATE cinemas SET cnc_id = ?, screens = ?, seats = ?, art_et_essai = ? WHERE id = ?",
            m.cnc.cnc_id,
            m.cnc.screens,
            m.cnc.seats,
            m.cnc.art_et_essai,
            m.cinema_id
        )
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(())
}

fn checked_integer(value: f64) -> anyhow::Result<i64> {
    anyhow::ensure!(
        value.is_finite()
            && value.fract() == 0.0
            && (0.0..9_223_372_036_854_775_808.0).contains(&value),
        "Nombre entier positif ou nul attendu : {value}"
    );

    Ok(value as i64)
}

#[cfg(test)]
mod tests {
    use super::*;

    const CNC_XLSX: &[u8] = include_bytes!("../../tests/fixtures/cnc.xlsx");

    #[test]
    fn inspect_cnc_fixture() {
        let mut workbook: Xlsx<_> = open_workbook_from_rs(Cursor::new(CNC_XLSX)).unwrap();
        let sheet = latest_year_sheet(&workbook.sheet_names()).unwrap();
        assert_eq!(sheet, "2025");

        let range = workbook.worksheet_range(&sheet).unwrap();

        // Coordonnées absolues, à partir de zéro : ligne Excel 5, colonne A.
        assert_eq!(
            range.get_value((4, 0)),
            Some(&calamine::Data::String("NAutoC".into()))
        );
    }

    #[test]
    fn test_parse_cnc_xlsx() {
        let cinemas = parse_cnc_xlsx(CNC_XLSX).expect("La fixture CNC doit être lisible");
        assert_eq!(cinemas.len(), 2060);
        assert!(cinemas.iter().all(|cinema| cinema.insee_code.len() == 5));

        let balzac = cinemas
            .iter()
            .find(|cinema| cinema.cnc_id == 35)
            .expect("Le Balzac doit être présent");
        assert_eq!(balzac.name, "BALZAC");
        assert_eq!(balzac.insee_code, "75108");
        assert_eq!(balzac.screens, 3);
        assert_eq!(balzac.seats, Some(589));
        assert!(balzac.art_et_essai);

        let opera = cinemas
            .iter()
            .find(|cinema| cinema.cnc_id == 204)
            .expect("UGC Opéra doit être présent");
        assert_eq!(opera.name, "UGC OPERA");
        assert!(!opera.art_et_essai);
    }

    #[test]
    fn test_latest_year_sheet() {
        let sheets = ["2023", "Notice", "2025", "2024"].map(String::from);

        assert_eq!(latest_year_sheet(&sheets).unwrap(), "2025");
    }

    #[test]
    fn test_latest_year_sheet_without_year() {
        let sheets = ["Notice"].map(String::from);

        assert!(latest_year_sheet(&sheets).is_err());
    }

    fn cinema(id: &str, name: &str, insee_code: &str) -> LocatedCinema {
        LocatedCinema {
            id: id.into(),
            name_search: normalize(name),
            insee_code: insee_code.into(),
        }
    }

    fn establishment(cnc_id: i64, name: &str, insee_code: &str) -> CncCinema {
        CncCinema {
            cnc_id,
            name: name.into(),
            insee_code: insee_code.into(),
            screens: 3,
            seats: Some(400),
            art_et_essai: true,
        }
    }

    fn matched_pairs(matches: &[CncMatch]) -> Vec<(String, i64)> {
        let mut pairs: Vec<_> = matches
            .iter()
            .map(|m| (m.cinema_id.to_owned(), m.cnc.cnc_id))
            .collect();
        pairs.sort();
        pairs
    }

    #[test]
    fn matches_names_within_the_same_commune() {
        let cinemas = [
            cinema("C0001", "UGC Opéra", "75109"),
            cinema("C0002", "Le Balzac", "75108"),
            cinema("C0003", "Pathé Wepler", "75118"),
            cinema("C0004", "Comoedia", "75118"),
        ];
        let cnc = [
            establishment(204, "UGC OPERA", "75109"),
            establishment(35, "BALZAC", "75108"),
            establishment(50, "PATHE WEPLER", "75118"),
            // Même nom que C0004, mais dans une autre commune : à ignorer.
            establishment(77, "COMOEDIA", "69123"),
        ];

        let matches = match_cinemas(&cinemas, &cnc);

        assert_eq!(
            matched_pairs(&matches),
            [
                ("C0001".into(), 204),
                ("C0002".into(), 35),
                ("C0003".into(), 50)
            ]
        );
    }

    #[test]
    fn closest_name_wins_a_disputed_establishment() {
        let cinemas = [
            cinema("C0001", "Olympia", "06029"),
            cinema("C0002", "Olympia Cannes - Salles 1 à 4", "06029"),
        ];
        let cnc = [establishment(10, "OLYMPIA", "06029")];

        let matches = match_cinemas(&cinemas, &cnc);

        assert_eq!(matched_pairs(&matches), [("C0001".into(), 10)]);
    }

    #[test]
    fn most_specific_name_wins_when_both_are_contained() {
        let cinemas = [
            cinema("P0036", "UGC Ciné Cité Lyon Part-Dieu", "69383"),
            cinema("P0671", "UGC Ciné Cité Internationale", "69386"),
        ];
        let cnc = [
            establishment(238461, "UGC CINE CITE", "69123"),
            establishment(725040, "UGC CINE CITE PART-DIEU", "69123"),
        ];

        let matches = match_cinemas(&cinemas, &cnc);

        assert_eq!(
            matched_pairs(&matches),
            [("P0036".into(), 725040), ("P0671".into(), 238461)]
        );
    }

    #[test]
    fn different_names_in_the_same_commune_are_not_matched() {
        let cinemas = [cinema("C0001", "Guillaume Apollinaire", "33063")];
        let cnc = [establishment(10, "CASINO JOA", "33063")];

        assert!(match_cinemas(&cinemas, &cnc).is_empty());
    }

    #[test]
    fn lyon_and_marseille_arrondissements_match_the_whole_commune() {
        let cinemas = [
            cinema("C0001", "Pathé Bellecour", "69382"),
            cinema("C0002", "Alhambra", "13216"),
        ];
        let cnc = [
            establishment(1, "PATHE BELLECOUR", "69123"),
            establishment(2, "ALHAMBRA", "13055"),
        ];

        let matches = match_cinemas(&cinemas, &cnc);

        assert_eq!(
            matched_pairs(&matches),
            [("C0001".into(), 1), ("C0002".into(), 2)]
        );
    }

    #[test]
    fn comparable_name_drops_generic_words_unless_nothing_is_left() {
        assert_eq!(comparable_name("Cinéma Le Balzac"), "balzac");
        assert_eq!(comparable_name("Ciné St-Michel"), "saint michel");
        assert_eq!(comparable_name("Cinéma"), "cinema");
    }

    #[test]
    fn similarity_accepts_names_completed_on_one_side() {
        assert_eq!(similarity("comoedia", "sete comoedia"), 1.0);
        assert!(similarity("saint paul", "saint paule") > MIN_SIMILARITY);
        assert!(similarity("rex", "olympia") < MIN_SIMILARITY);
    }

    /// `id`, `cnc_id`, `screens`, `seats`, `art_et_essai`.
    type SavedRow = (String, Option<i64>, Option<i64>, Option<i64>, bool);

    #[tokio::test]
    async fn save_matches_replaces_previous_cnc_data() {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!().run(&pool).await.unwrap();
        sqlx::query(
            "INSERT INTO cinemas (id, name, name_search, updated_at, cnc_id, screens, seats, art_et_essai)
             VALUES ('OLD', 'Ancien', 'ancien', datetime('now'), 1, 2, 100, 1),
                    ('NEW', 'Nouveau', 'nouveau', datetime('now'), NULL, NULL, NULL, 0)",
        )
        .execute(&pool)
        .await
        .unwrap();
        let cnc = establishment(35, "BALZAC", "75108");

        save_matches(
            &pool,
            &[CncMatch {
                cinema_id: "NEW",
                cnc: &cnc,
                score: 1.0,
            }],
        )
        .await
        .unwrap();

        let rows: Vec<SavedRow> = sqlx::query_as(
            "SELECT id, cnc_id, screens, seats, art_et_essai FROM cinemas ORDER BY id",
        )
        .fetch_all(&pool)
        .await
        .unwrap();
        assert_eq!(
            rows,
            [
                ("NEW".into(), Some(35), Some(3), Some(400), true),
                ("OLD".into(), None, None, None, false),
            ]
        );
    }
}
