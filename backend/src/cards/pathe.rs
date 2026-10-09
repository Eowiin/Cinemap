//! Pathé CinéPass, réseau Pathé : l'API JSON de pathe.fr (les partenaires, absents du
//! JSON, sont dans `docs/data/cartes.csv`).

use serde::Deserialize;
use tracing::warn;

use super::CardCinema;

#[derive(Deserialize)]
struct PatheCinema {
    name: String,
    /// `false` pour un cinéma fermé ou en travaux (Le Cézanne à Aix en 2026-10).
    status: bool,
    theaters: Vec<Theater>,
}

/// Un cinéma peut avoir plusieurs `theaters` (Pathé Parnasse : 3), tous à la même
/// adresse dans la fixture : on prend le premier.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Theater {
    address_zip: String,
    address_city: String,
    gps_position: Option<GpsPosition>,
}

/// Piège : `x` est la **latitude** et `y` la longitude.
#[derive(Deserialize)]
struct GpsPosition {
    #[serde(rename = "x")]
    lat: f64,
    #[serde(rename = "y")]
    lng: f64,
}

pub fn parse_pathe(json: &str) -> anyhow::Result<Vec<CardCinema>> {
    let cinemas: Vec<PatheCinema> = serde_json::from_str(json)?;
    let mut entries = Vec::with_capacity(cinemas.len());
    for cinema in cinemas.into_iter().filter(|c| c.status) {
        let Some(theater) = cinema.theaters.into_iter().next() else {
            warn!(name = cinema.name, "Cinéma Pathé sans adresse, ignoré");
            continue;
        };
        entries.push(CardCinema {
            name: cinema.name,
            postal_code: theater.address_zip,
            city: theater.address_city,
            position: theater.gps_position.map(|p| (p.lat, p.lng)),
        });
    }
    anyhow::ensure!(
        !entries.is_empty(),
        "Aucun cinéma ouvert dans la réponse Pathé"
    );
    Ok(entries)
}

#[cfg(test)]
mod tests {
    use super::*;

    const PATHE_JSON: &str = include_str!("../../tests/fixtures/pathe-cinemas-2026-10-09.json");

    #[test]
    fn parses_open_cinemas_with_latitude_first() {
        let cinemas = parse_pathe(PATHE_JSON).unwrap();

        assert_eq!(cinemas.len(), 77);
        assert!(cinemas.iter().all(|c| c.name != "Le Cézanne"));
        assert!(cinemas.iter().all(|c| c.position.is_some()));
        let angers = cinemas.iter().find(|c| c.name == "Pathé Angers").unwrap();
        assert_eq!(angers.position, Some((47.479464, -0.550977)));
        assert_eq!(angers.postal_code, "49100");
    }
}
