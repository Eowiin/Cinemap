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
        #[arg(long, conflicts_with = "days")]
        date: Option<String>,
        /// Nombre de jours à partir d'aujourd'hui (7 = la semaine, J à J+6).
        #[arg(long, value_parser = clap::value_parser!(u32).range(1..=7))]
        days: Option<u32>,
        /// Run de nuit : alimente « mis à jour il y a X » (`/api/meta`) même limité à des
        /// départements.
        #[arg(long, conflicts_with = "cinema")]
        nightly: bool,
        /// Limite le scraping aux cinémas de ces départements (ex. 75 ou 75,92,93).
        #[arg(long, conflicts_with = "cinema", value_delimiter = ',')]
        department: Vec<String>,
        /// Intervalle minimal entre deux requêtes AlloCiné, en millisecondes (défaut : 333,
        /// soit 3 req/s). Sert de rythme de départ et de plancher.
        #[arg(long, value_parser = clap::value_parser!(u64).range(333..))]
        interval_ms: Option<u64>,
    },
    /// Enrichit les films à l'affiche avec TMDB (image de fond, bande-annonce, note).
    Tmdb {
        /// Retraite aussi les films synchronisés il y a moins de 7 jours.
        #[arg(long)]
        all: bool,
    },
    /// Importe les listes des cartes d'abonnement (UGC Illimité, Pathé CinéPass) et les
    /// lignes de docs/data/cartes.csv.
    ImportCards {
        /// Accepte une liste deux fois plus courte que la précédente (le garde-fou la refuse).
        #[arg(long)]
        force: bool,
    },
    /// Lance l'API HTTP.
    Serve {
        /// Adresse d'écoute (en prod, nginx est devant : inutile d'exposer le port).
        #[arg(long, env = "BIND_ADDR", default_value = "127.0.0.1:3000")]
        addr: String,
    },
}

#[derive(Parser)]
#[command(version, about, long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: SubCommands,
}
