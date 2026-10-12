//! Villes : les cinémas visibles regroupés par (`city`, département). Pas de table : la
//! liste se déduit des cinémas et ne change qu'avec l'import hebdomadaire, d'où un index
//! gardé en mémoire quelques minutes plutôt qu'un `GROUP BY` à chaque requête.

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use axum::{
    Json,
    extract::{Path, State},
};
use sqlx::{SqlitePool, types::Json as SqlJson};

use super::AppState;
use super::cinemas::CinemaRow;
use super::error::{ApiResult, AppError};
use super::geo::Position;
use super::types::{City, CitySummary};
use crate::text::slugify;

/// Au-delà, l'index est recalculé à la requête suivante (~2 600 villes, quelques ms).
const TTL: Duration = Duration::from_secs(10 * 60);

#[derive(Debug, Clone)]
pub struct CityEntry {
    pub slug: String,
    pub name: String,
    /// Code INSEE (« 69 », « 2A ») tel qu'en base, pour retrouver les cinémas.
    pub department_code: Option<String>,
    pub department: Option<String>,
    pub label: String,
    pub cinema_count: i64,
    pub center: Position,
}

impl CityEntry {
    pub fn summary(&self) -> CitySummary {
        CitySummary {
            slug: self.slug.clone(),
            name: self.name.clone(),
            label: self.label.clone(),
            department: self.department.clone(),
            cinema_count: self.cinema_count,
        }
    }
}

#[derive(Debug, Default)]
pub struct CityIndex {
    /// Les plus fournies d'abord (ordre de la recherche).
    pub cities: Vec<CityEntry>,
    by_slug: HashMap<String, usize>,
    by_key: HashMap<(String, Option<String>), usize>,
}

impl CityIndex {
    pub fn get(&self, slug: &str) -> Option<&CityEntry> {
        self.by_slug.get(slug).map(|&i| &self.cities[i])
    }

    /// Slug de la ville d'un cinéma (même nom de ville, même département).
    pub fn slug_of(&self, city: &str, department_code: Option<&str>) -> Option<&str> {
        self.by_key
            .get(&(city.to_owned(), department_code.map(str::to_owned)))
            .map(|&i| self.cities[i].slug.as_str())
    }

    /// Villes dont le nom commence par `q` (déjà normalisé), les plus fournies d'abord.
    pub fn search(&self, q: &str, limit: usize) -> Vec<&CityEntry> {
        let q = slugify(q);
        if q.is_empty() {
            return Vec::new();
        }
        self.cities
            .iter()
            .filter(|c| c.slug.starts_with(&q))
            .take(limit)
            .collect()
    }

    /// `departments` : code INSEE → nom. Les lignes : (ville, code, nombre de cinémas, centre).
    pub fn build(
        rows: Vec<(String, Option<String>, i64, Position)>,
        departments: &HashMap<String, String>,
    ) -> Self {
        let mut rows: Vec<_> = rows
            .into_iter()
            .map(|row| (slugify(&row.0), row))
            .filter(|(slug, _)| !slug.is_empty())
            .collect();
        // Plus de cinémas d'abord : à slug égal malgré le département, la plus grande gagne.
        rows.sort_by(|a, b| b.1.2.cmp(&a.1.2).then_with(|| a.0.cmp(&b.0)));

        let mut homonyms: HashMap<&str, usize> = HashMap::new();
        for (slug, _) in &rows {
            *homonyms.entry(slug.as_str()).or_default() += 1;
        }
        let homonyms: HashMap<String, usize> = homonyms
            .into_iter()
            .map(|(slug, n)| (slug.to_owned(), n))
            .collect();

        let mut index = CityIndex::default();
        for (base, (name, code, cinema_count, center)) in rows {
            let department = code.as_ref().and_then(|c| departments.get(c).cloned());
            let ambiguous = homonyms[&base] > 1;
            let slug = match (&code, ambiguous) {
                (Some(code), true) => format!("{base}-{}", code.to_ascii_lowercase()),
                _ => base,
            };
            if index.by_slug.contains_key(&slug) {
                // Deux graphies d'une même ville (« Saint-Denis » / « Saint Denis ») :
                // la première, la plus fournie, garde le slug.
                continue;
            }
            let label = match (&department, ambiguous) {
                (Some(department), true) => format!("{name} ({department})"),
                _ => name.clone(),
            };
            let i = index.cities.len();
            index.by_slug.insert(slug.clone(), i);
            index.by_key.insert((name.clone(), code.clone()), i);
            index.cities.push(CityEntry {
                slug,
                name,
                department_code: code,
                department,
                label,
                cinema_count,
                center,
            });
        }
        index
    }
}

struct CityRow {
    city: String,
    department: Option<String>,
    cinema_count: i64,
    lat: f64,
    lng: f64,
}

/// Index partagé par toutes les requêtes, recalculé quand il a plus de `TTL`.
#[derive(Default)]
pub struct CityCache {
    current: Mutex<Option<(Instant, Arc<CityIndex>)>>,
}

impl CityCache {
    pub async fn get(
        &self,
        pool: &SqlitePool,
        departments: &HashMap<String, String>,
    ) -> ApiResult<Arc<CityIndex>> {
        if let Some((built, index)) = &*self.current.lock().expect("verrou des villes")
            && built.elapsed() < TTL
        {
            return Ok(Arc::clone(index));
        }
        // Pas de verrou pendant la requête : deux requêtes simultanées peuvent recalculer
        // chacune l'index, c'est sans conséquence.
        let rows = sqlx::query_as!(
            CityRow,
            r#"SELECT city AS "city!: String", department AS "department?: String",
                      count(*) AS "cinema_count!: i64",
                      avg(lat) AS "lat!: f64", avg(lng) AS "lng!: f64"
               FROM visible_cinemas
               WHERE city IS NOT NULL
               GROUP BY city, department"#,
        )
        .fetch_all(pool)
        .await?;
        let rows = rows
            .into_iter()
            .map(|r| {
                (
                    r.city,
                    r.department,
                    r.cinema_count,
                    Position {
                        lat: r.lat,
                        lng: r.lng,
                    },
                )
            })
            .collect();
        let index = Arc::new(CityIndex::build(rows, departments));
        *self.current.lock().expect("verrou des villes") =
            Some((Instant::now(), Arc::clone(&index)));
        Ok(index)
    }
}

/// Une ville et ses cinémas ; 404 si le slug est inconnu.
pub(super) async fn fetch_city(state: &AppState, slug: &str) -> ApiResult<City> {
    let index = state.cities.get(&state.pool, &state.departments).await?;
    let entry = index
        .get(slug)
        .ok_or(AppError::NotFound("Ville introuvable"))?;
    let rows = sqlx::query_as!(
        CinemaRow,
        r#"SELECT c.id AS "id!: String", c.name AS "name!: String", c.city AS "city?: String",
                  c.lat AS "lat!: f64", c.lng AS "lng!: f64",
                  c.art_et_essai AS "art_et_essai!: bool",
                  (SELECT json_group_array(card_id) FROM cinema_cards WHERE cinema_id = c.id)
                    AS "cards!: SqlJson<Vec<String>>"
           FROM visible_cinemas c
           WHERE c.city = ?1 AND c.department IS ?2
           ORDER BY c.name_search"#,
        entry.name,
        entry.department_code,
    )
    .fetch_all(&state.pool)
    .await?;
    Ok(City {
        summary: entry.summary(),
        lat: entry.center.lat,
        lng: entry.center.lng,
        cinemas: rows.into_iter().map(|row| row.into_summary(None)).collect(),
    })
}

pub async fn get_one(
    State(state): State<AppState>,
    Path(slug): Path<String>,
) -> ApiResult<Json<City>> {
    Ok(Json(fetch_city(&state, &slug).await?))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(city: &str, code: &str, n: i64) -> (String, Option<String>, i64, Position) {
        (
            city.into(),
            Some(code.into()),
            n,
            Position { lat: 0.0, lng: 0.0 },
        )
    }

    #[test]
    fn homonyms_get_their_department_in_slug_and_label() {
        let departments = HashMap::from([
            ("93".to_owned(), "Seine-Saint-Denis".to_owned()),
            ("974".to_owned(), "La Réunion".to_owned()),
            ("69".to_owned(), "Rhône".to_owned()),
        ]);
        let index = CityIndex::build(
            vec![
                row("Saint-Denis", "93", 3),
                row("Saint-Denis", "974", 2),
                row("Lyon", "69", 17),
            ],
            &departments,
        );
        let lyon = index.get("lyon").unwrap();
        assert_eq!(lyon.label, "Lyon");
        let reunion = index.get("saint-denis-974").unwrap();
        assert_eq!(reunion.label, "Saint-Denis (La Réunion)");
        assert!(index.get("saint-denis").is_none());
        assert_eq!(
            index.slug_of("Saint-Denis", Some("93")),
            Some("saint-denis-93")
        );
    }

    #[test]
    fn search_matches_the_start_of_the_name() {
        let index = CityIndex::build(
            vec![
                row("Lyon", "69", 17),
                row("Villeurbanne", "69", 2),
                row("Saint-Denis", "93", 3),
            ],
            &HashMap::new(),
        );
        let slugs =
            |q| -> Vec<String> { index.search(q, 5).iter().map(|c| c.slug.clone()).collect() };
        assert_eq!(slugs("lyo"), ["lyon"]);
        assert_eq!(slugs("saint den"), ["saint-denis"]);
        assert!(slugs("urbanne").is_empty());
    }
}
