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
    Scrape,
    Serve,
}

#[derive(Parser)]
#[command(version, about, long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: SubCommands,
}
