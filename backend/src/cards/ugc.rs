//! UGC Illimité : une page HTML statique, toutes régions sur une seule page.

use scraper::{ElementRef, Html, Selector};
use tracing::warn;

use super::CardCinema;
use crate::text::get_postal_code;

/// Chaque cinéma est un bloc `div.item--cinema-content` : le nom dans `div.color--white`,
/// l'adresse dans `div.color--blue-grey`, terminée par `75012&nbsp;PARIS`.
pub fn parse_ugc(html: &str) -> anyhow::Result<Vec<CardCinema>> {
    let document = Html::parse_document(html);
    let block = Selector::parse("div.item--cinema-content").expect("Sélecteur CSS invalide");
    let name = Selector::parse("div.color--white").expect("Sélecteur CSS invalide");
    let address = Selector::parse("div.color--blue-grey").expect("Sélecteur CSS invalide");

    let mut cinemas = Vec::new();
    for item in document.select(&block) {
        let Some(name) = item.select(&name).next().map(text_of) else {
            warn!("Bloc UGC sans nom, ignoré");
            continue;
        };
        let address = item
            .select(&address)
            .next()
            .map(text_of)
            .unwrap_or_default();
        let Some(start) = get_postal_code(&address) else {
            warn!(name, address, "Cinéma UGC sans code postal, ignoré");
            continue;
        };
        cinemas.push(CardCinema {
            name,
            postal_code: address[start..start + 5].to_owned(),
            city: address[start + 5..].trim().to_owned(),
            position: None,
        });
    }
    anyhow::ensure!(
        !cinemas.is_empty(),
        "Aucun cinéma dans la page UGC (structure changée ?)"
    );
    Ok(cinemas)
}

/// Texte d'un élément, espaces (dont `&nbsp;` et sauts de ligne) réduits à une seule.
fn text_of(element: ElementRef) -> String {
    element
        .text()
        .flat_map(str::split_whitespace)
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    const UGC_HTML: &str = include_str!("../../tests/fixtures/ugc-illimite-2026-10-09.html");

    fn find<'a>(cinemas: &'a [CardCinema], name: &str) -> &'a CardCinema {
        cinemas
            .iter()
            .find(|c| c.name == name)
            .unwrap_or_else(|| panic!("{name} absent"))
    }

    #[test]
    fn parses_every_cinema_of_the_fixture() {
        let cinemas = parse_ugc(UGC_HTML).unwrap();

        assert_eq!(cinemas.len(), 145);
        assert_eq!(
            cinemas
                .iter()
                .filter(|c| c.postal_code.starts_with("75"))
                .count(),
            59
        );

        let halles = find(&cinemas, "UGC Ciné Cité Les Halles");
        assert_eq!(
            (halles.postal_code.as_str(), halles.city.as_str()),
            ("75001", "PARIS")
        );
        assert_eq!(find(&cinemas, "MK2 BEAUBOURG").postal_code, "75003");
        let ester = find(&cinemas, "GRAND ECRAN ESTER");
        assert_eq!(
            (ester.postal_code.as_str(), ester.city.as_str()),
            ("87100", "LIMOGES")
        );
        let noisy = find(&cinemas, "UGC Ciné Cité Noisy-le-Grand");
        // Code CEDEX, sans le mot « CEDEX » : le repli par ville le rattrape.
        assert_eq!(
            (noisy.postal_code.as_str(), noisy.city.as_str()),
            ("93193", "NOISY-LE-GRAND")
        );
    }

    #[test]
    fn a_page_without_cinemas_is_an_error() {
        assert!(parse_ugc("<html><body>Maintenance</body></html>").is_err());
    }
}
