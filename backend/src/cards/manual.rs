//! Ajouts à la main (`docs/data/cartes.csv`) : partenaires sans adresse exploitable
//! (PDF Pathé), entrées que le croisement automatique ne trouve pas.

use serde::Deserialize;

use super::CARDS;

/// Embarqué dans le binaire, comme les départements : rien à déployer à côté.
const CSV: &str = include_str!("../../../docs/data/cartes.csv");

#[derive(Debug, Deserialize)]
pub struct ManualLink {
    pub card_id: String,
    /// Vide : l'entrée `source_name` n'est pas sur AlloCiné (connue, plus signalée).
    pub cinema_id: Option<String>,
    /// Nom de l'entrée dans la source automatique que cette ligne remplace : elle
    /// n'est alors plus signalée comme non croisée. Vide pour un partenaire hors source.
    pub source_name: Option<String>,
    // La colonne `commentaire` n'est lue que par les humains (serde ignore les colonnes en trop).
}

/// Lignes du CSV ; une carte inconnue est une erreur (le CSV est testé à la compilation).
pub fn manual_links() -> anyhow::Result<Vec<ManualLink>> {
    let mut links = Vec::new();
    for row in csv::Reader::from_reader(CSV.as_bytes()).deserialize() {
        let link: ManualLink = row?;
        anyhow::ensure!(
            CARDS.iter().any(|card| card.id == link.card_id),
            "cartes.csv : carte inconnue {}",
            link.card_id
        );
        links.push(link);
    }
    Ok(links)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_embedded_csv_is_valid() {
        let links = manual_links().unwrap();
        assert!(!links.is_empty());
        // Une ligne sans cinéma ne sert qu'à faire taire une entrée de la source.
        assert!(
            links
                .iter()
                .all(|l| l.cinema_id.is_some() || l.source_name.is_some())
        );
    }
}
