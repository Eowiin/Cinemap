use std::io::Cursor;

use anyhow::{Context, anyhow};
use calamine::{RangeDeserializerBuilder, Reader, Xlsx, open_workbook_from_rs};
use serde::Deserialize;

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
}
