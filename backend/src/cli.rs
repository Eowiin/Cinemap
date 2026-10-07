use clap::{Parser, Subcommand};

#[derive(Subcommand)]
pub enum SubCommands {
    ImportCinemas,
    /// Géocode les adresses en base, sans scraping AlloCiné.
    Geocode {
        /// Recalcule aussi les positions déjà enregistrées.
        #[arg(long)]
        all: bool,
    },
    /// Ajoute écrans, fauteuils et label Art et Essai depuis le fichier du CNC, sans scraping.
    EnrichCnc,
    /// Scrape les séances AlloCiné pour un cinéma, un département ou tous les cinémas visibles.
    Scrape {
        /// Identifiant AlloCiné du cinéma à scraper.
        #[arg(long, conflicts_with = "department")]
        cinema: Option<String>,
        /// Date cinéma au format YYYY-MM-DD (par défaut : J, J+1, J+2 et J+6 le mercredi).
        #[arg(long)]
        date: Option<String>,
        /// Limite le scraping aux cinémas de ce département (ex. 75).
        #[arg(long, conflicts_with = "cinema")]
        department: Option<String>,
    },
    Serve,
}

#[derive(Parser)]
#[command(version, about, long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: SubCommands,
}
