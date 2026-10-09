//! Cartes d'abonnement (UGC Illimité, Pathé CinéPass) : dans quels cinémas sont-elles
//! acceptées ? Feuille de route : `docs/CARTES.md`.
//!
//! Pour chaque carte : liste téléchargée et lue (fonction pure par source), croisée
//! avec nos cinémas, puis enregistrée en une transaction avec les lignes de
//! `docs/data/cartes.csv`. Un garde-fou garde les anciens liens si la liste paraît cassée.

pub mod manual;
pub mod matching;
pub mod pathe;
pub mod ugc;

use std::collections::HashSet;

use anyhow::Context;
use reqwest::Client;
use sqlx::SqlitePool;
use tracing::{debug, info, warn};

use crate::client::{RETRY_DELAYS, fetch_with_retries};
use manual::{ManualLink, manual_links};
use matching::{CardMatch, CardTarget, How, match_by_distance, match_by_postal_code};

/// Un cinéma tel que la liste d'une carte le décrit.
#[derive(Debug, Clone, PartialEq)]
pub struct CardCinema {
    pub name: String,
    pub postal_code: String,
    pub city: String,
    /// `(lat, lng)`, quand la source la donne (Pathé).
    pub position: Option<(f64, f64)>,
}

#[derive(Debug, Clone, Copy)]
enum Source {
    Ugc,
    Pathe,
}

#[derive(Debug)]
pub struct Card {
    pub id: &'static str,
    pub name: &'static str,
    pub source_url: &'static str,
    source: Source,
}

/// Ajouter une carte : une entrée ici (et un module de source), ou des lignes dans le CSV.
pub const CARDS: &[Card] = &[
    Card {
        id: "ugc_illimite",
        name: "UGC Illimité",
        source_url: "https://www.ugc.fr/cinemas-acceptant-ui.html",
        source: Source::Ugc,
    },
    Card {
        id: "pathe_cinepass",
        name: "Pathé CinéPass",
        source_url: "https://www.pathe.fr/api/cinemas",
        source: Source::Pathe,
    },
];

/// Importe toutes les cartes. Une source en erreur n'empêche pas les autres (ni les
/// lignes manuelles) d'être enregistrées, mais la commande finit en erreur pour le cron.
/// `force` : accepte une liste beaucoup plus courte que la précédente (voir `save_card`).
pub async fn import_cards(pool: &SqlitePool, client: &Client, force: bool) -> anyhow::Result<()> {
    let cinemas = sqlx::query_as!(
        CardTarget,
        r#"SELECT id AS "id!", name, postal_code, city, department, lat, lng FROM cinemas"#
    )
    .fetch_all(pool)
    .await?;
    let manual = manual_links()?;
    let mut failed = Vec::new();

    for card in CARDS {
        let card_manual: Vec<&ManualLink> = manual
            .iter()
            .filter(|link| link.card_id == card.id)
            .collect();
        // Gardé jusqu'à la fin du tour : les liens empruntent les entrées et les cinémas.
        let fetched = fetch_entries(client, card).await;
        let auto = match &fetched {
            Ok(entries) => {
                let matches = match card.source {
                    Source::Ugc => match_by_postal_code(&cinemas, entries),
                    Source::Pathe => match_by_distance(&cinemas, entries),
                };
                report(card, entries, &matches, &card_manual);
                Some(matches.iter().map(|m| m.cinema_id).collect::<Vec<_>>())
            }
            Err(error) => {
                warn!(
                    card = card.id,
                    error = format!("{error:#}"),
                    "Liste de la carte indisponible, anciens liens gardés"
                );
                failed.push(card.id);
                None
            }
        };
        let manual_ids: Vec<&str> = card_manual
            .iter()
            .filter_map(|l| l.cinema_id.as_deref())
            .collect();
        let saved = save_card(pool, card, auto.as_deref(), &manual_ids, force).await?;
        info!(
            card = card.id,
            auto = saved.auto,
            auto_remplaces = saved.replaced,
            manuels = saved.manual,
            "Carte enregistrée"
        );
    }
    anyhow::ensure!(
        failed.is_empty(),
        "Cartes en erreur : {}",
        failed.join(", ")
    );
    Ok(())
}

async fn fetch_entries(client: &Client, card: &Card) -> anyhow::Result<Vec<CardCinema>> {
    let bytes = fetch_with_retries(card.source_url, &RETRY_DELAYS, || {
        client.get(card.source_url)
    })
    .await?;
    let body = std::str::from_utf8(&bytes).context("Réponse non UTF-8")?;
    match card.source {
        Source::Ugc => ugc::parse_ugc(body),
        Source::Pathe => pathe::parse_pathe(body),
    }
}

/// Log de relecture : chaque entrée non croisée (sauf celles reprises dans le CSV), et
/// chaque croisement de seconde passe (même ville, ou plus de 500 m).
fn report(card: &Card, entries: &[CardCinema], matches: &[CardMatch], manual: &[&ManualLink]) {
    let covered: HashSet<&str> = manual
        .iter()
        .filter_map(|l| l.source_name.as_deref())
        .collect();
    let mut unmatched = 0;
    for entry in entries {
        // `ptr::eq` : la même entrée de la liste (deux entrées peuvent avoir le même nom).
        let matched = matches.iter().any(|m| std::ptr::eq(m.entry, entry));
        if matched || covered.contains(entry.name.as_str()) {
            continue;
        }
        unmatched += 1;
        warn!(
            card = card.id,
            name = entry.name,
            postal_code = entry.postal_code,
            city = entry.city,
            "Cinéma de la carte non croisé (à ajouter dans docs/data/cartes.csv ?)"
        );
    }
    for m in matches {
        debug!(
            card = card.id,
            cinema = m.cinema_id,
            name = m.entry.name,
            score = m.score,
            "Croisement"
        );
        let doubtful = match m.how {
            How::PostalCode => false,
            How::City => true,
            How::Distance(meters) => meters > matching::NEAR_M,
        };
        if doubtful {
            info!(
                card = card.id,
                cinema = m.cinema_id,
                name = m.entry.name,
                how = ?m.how,
                score = m.score,
                "Croisement à relire"
            );
        }
    }
    info!(
        card = card.id,
        entrees = entries.len(),
        croisees = matches.len(),
        non_croisees = unmatched,
        "Liste de la carte croisée"
    );
}

#[derive(Debug, PartialEq)]
pub struct Saved {
    /// Liens automatiques en base après l'enregistrement.
    pub auto: i64,
    /// `false` : liste absente, vide ou trop courte, anciens liens automatiques gardés.
    pub replaced: bool,
    pub manual: usize,
}

/// Enregistre les liens d'une carte en une transaction.
///
/// Garde-fou : une liste vide, ou **moins de la moitié** des liens automatiques
/// actuels, garde les anciens (une page qui change de structure ne doit pas effacer la
/// carte du site en silence). `force` accepte quand même une liste courte, jamais une
/// liste vide. Les lignes manuelles sont toujours réappliquées.
pub async fn save_card(
    pool: &SqlitePool,
    card: &Card,
    auto: Option<&[&str]>,
    manual: &[&str],
    force: bool,
) -> anyhow::Result<Saved> {
    let mut tx = pool.begin().await?;
    // La ligne doit exister avant les liens (clé étrangère). `updated_at` n'est avancé
    // qu'après un import automatique accepté.
    sqlx::query!(
        "INSERT INTO cards (id, name, source_url, updated_at) VALUES (?, ?, ?, datetime('now'))
         ON CONFLICT (id) DO UPDATE SET name = excluded.name, source_url = excluded.source_url",
        card.id,
        card.name,
        card.source_url
    )
    .execute(&mut *tx)
    .await?;
    let current = sqlx::query_scalar!(
        "SELECT count(*) FROM cinema_cards WHERE card_id = ? AND manual = 0",
        card.id
    )
    .fetch_one(&mut *tx)
    .await?;

    let accepted = match auto {
        None => None,
        Some([]) => {
            warn!(card = card.id, "Aucun cinéma croisé, anciens liens gardés");
            None
        }
        Some(ids) if !force && (ids.len() as i64) * 2 < current => {
            warn!(
                card = card.id,
                nouveaux = ids.len(),
                actuels = current,
                "Liste deux fois plus courte que la précédente, anciens liens gardés \
                 (import-cards --force pour l'accepter)"
            );
            None
        }
        Some(ids) => Some(ids),
    };

    if let Some(ids) = accepted {
        sqlx::query!(
            "DELETE FROM cinema_cards WHERE card_id = ? AND manual = 0",
            card.id
        )
        .execute(&mut *tx)
        .await?;
        for id in ids {
            // Une ligne manuelle pour le même cinéma devient automatique.
            sqlx::query!(
                "INSERT INTO cinema_cards (cinema_id, card_id, manual) VALUES (?, ?, 0)
                 ON CONFLICT (cinema_id, card_id) DO UPDATE SET manual = 0",
                id,
                card.id
            )
            .execute(&mut *tx)
            .await?;
        }
        sqlx::query!(
            "UPDATE cards SET updated_at = datetime('now') WHERE id = ?",
            card.id
        )
        .execute(&mut *tx)
        .await?;
    }

    sqlx::query!(
        "DELETE FROM cinema_cards WHERE card_id = ? AND manual = 1",
        card.id
    )
    .execute(&mut *tx)
    .await?;
    let mut manual_saved = 0;
    for id in manual {
        let known = sqlx::query_scalar!(
            r#"SELECT EXISTS (SELECT 1 FROM cinemas WHERE id = ?) AS "known!: bool""#,
            id
        )
        .fetch_one(&mut *tx)
        .await?;
        if !known {
            warn!(
                card = card.id,
                cinema = id,
                "cartes.csv : cinéma inconnu, ligne ignorée"
            );
            continue;
        }
        // Déjà lié automatiquement : la ligne manuelle ne change rien.
        sqlx::query!(
            "INSERT INTO cinema_cards (cinema_id, card_id, manual) VALUES (?, ?, 1)
             ON CONFLICT (cinema_id, card_id) DO NOTHING",
            id,
            card.id
        )
        .execute(&mut *tx)
        .await?;
        manual_saved += 1;
    }

    let auto_count = sqlx::query_scalar!(
        "SELECT count(*) FROM cinema_cards WHERE card_id = ? AND manual = 0",
        card.id
    )
    .fetch_one(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(Saved {
        auto: auto_count,
        replaced: accepted.is_some(),
        manual: manual_saved,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn pool() -> SqlitePool {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!().run(&pool).await.unwrap();
        sqlx::query(
            "INSERT INTO cinemas (id, name, name_search, updated_at)
             VALUES ('A', 'A', 'a', datetime('now')), ('B', 'B', 'b', datetime('now')),
                    ('C', 'C', 'c', datetime('now')), ('D', 'D', 'd', datetime('now'))",
        )
        .execute(&pool)
        .await
        .unwrap();
        pool
    }

    async fn links(pool: &SqlitePool) -> Vec<(String, bool)> {
        sqlx::query_as("SELECT cinema_id, manual FROM cinema_cards ORDER BY cinema_id")
            .fetch_all(pool)
            .await
            .unwrap()
    }

    const UGC: &Card = &CARDS[0];

    #[tokio::test]
    async fn replaces_automatic_links_and_reapplies_manual_ones() {
        let pool = pool().await;
        save_card(&pool, UGC, Some(&["A", "B"]), &["D"], false)
            .await
            .unwrap();

        let saved = save_card(&pool, UGC, Some(&["A", "C"]), &["D", "INCONNU"], false)
            .await
            .unwrap();

        assert_eq!(
            saved,
            Saved {
                auto: 2,
                replaced: true,
                manual: 1
            }
        );
        assert_eq!(
            links(&pool).await,
            [("A".into(), false), ("C".into(), false), ("D".into(), true)]
        );
    }

    #[tokio::test]
    async fn guard_keeps_links_when_the_list_is_empty_or_much_shorter() {
        let pool = pool().await;
        save_card(&pool, UGC, Some(&["A", "B", "C"]), &[], false)
            .await
            .unwrap();

        for auto in [Some(&[][..]), Some(&["A"][..]), None] {
            let saved = save_card(&pool, UGC, auto, &[], false).await.unwrap();
            assert!(!saved.replaced);
            assert_eq!(saved.auto, 3);
        }

        // Une liste vide reste refusée même avec `force`, une liste courte passe.
        assert!(
            !save_card(&pool, UGC, Some(&[]), &[], true)
                .await
                .unwrap()
                .replaced
        );
        let saved = save_card(&pool, UGC, Some(&["A"]), &[], true)
            .await
            .unwrap();
        assert_eq!((saved.replaced, saved.auto), (true, 1));
    }

    #[tokio::test]
    async fn deleting_a_cinema_deletes_its_links() {
        let pool = pool().await;
        save_card(&pool, UGC, Some(&["A", "B"]), &[], false)
            .await
            .unwrap();

        sqlx::query("DELETE FROM cinemas WHERE id = 'A'")
            .execute(&pool)
            .await
            .unwrap();

        assert_eq!(links(&pool).await, [("B".into(), false)]);
    }
}
