//! Mesure d'audience : des compteurs par jour (`daily_events`), alimentés par le front
//! (`POST /api/events`) et lus par `cinemap stats`.
//!
//! Rien ne permet de reconnaître un visiteur : pas de cookie, pas d'IP, pas d'identifiant,
//! seulement « tant de réservations pour ce cinéma ce jour-là ».

use chrono::Days;
use sqlx::SqlitePool;

use crate::time::{DATE_FORMAT, paris_today};

/// Ce que le front peut compter. La liste est fermée : un nom inconnu est refusé.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Event {
    ViewHome,
    ViewCity,
    ViewMovie,
    ViewCinema,
    ViewProgramme,
    /// Clic sur « Réserver » (lien sortant vers la billetterie du cinéma).
    Book,
    /// Séance ajoutée à Mon programme.
    Add,
    /// Journée composée automatiquement et gardée.
    Compose,
    /// Programme partagé (partage natif ou lien copié).
    Share,
    /// Programme exporté vers l'agenda (`.ics`).
    Calendar,
    /// Programme partagé par quelqu'un d'autre, gardé dans le sien.
    KeepShared,
}

/// Ce que désigne la cible d'un événement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    None,
    Cinema,
    Movie,
    City,
}

impl Event {
    pub const ALL: [Event; 11] = [
        Event::ViewHome,
        Event::ViewCity,
        Event::ViewMovie,
        Event::ViewCinema,
        Event::ViewProgramme,
        Event::Book,
        Event::Add,
        Event::Compose,
        Event::Share,
        Event::Calendar,
        Event::KeepShared,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Event::ViewHome => "view_home",
            Event::ViewCity => "view_city",
            Event::ViewMovie => "view_movie",
            Event::ViewCinema => "view_cinema",
            Event::ViewProgramme => "view_programme",
            Event::Book => "book",
            Event::Add => "add",
            Event::Compose => "compose",
            Event::Share => "share",
            Event::Calendar => "calendar",
            Event::KeepShared => "keep_shared",
        }
    }

    pub fn parse(name: &str) -> Option<Event> {
        Event::ALL.into_iter().find(|e| e.name() == name)
    }

    pub fn target(self) -> Target {
        match self {
            Event::ViewCity => Target::City,
            Event::ViewMovie => Target::Movie,
            Event::ViewCinema | Event::Book | Event::Add => Target::Cinema,
            _ => Target::None,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Event::ViewHome => "accueil",
            Event::ViewCity => "pages ville",
            Event::ViewMovie => "fiches film",
            Event::ViewCinema => "pages cinéma",
            Event::ViewProgramme => "Mon programme",
            Event::Book => "clics « Réserver »",
            Event::Add => "séances ajoutées",
            Event::Compose => "journées composées",
            Event::Share => "programmes partagés",
            Event::Calendar => "exports agenda",
            Event::KeepShared => "programmes reçus gardés",
        }
    }
}

/// Ajoute 1 au compteur du jour. `target` est déjà validée par l'appelant ('' sans cible).
pub async fn record(pool: &SqlitePool, event: Event, target: &str) -> sqlx::Result<()> {
    let day = paris_today().format(DATE_FORMAT).to_string();
    let name = event.name();
    sqlx::query!(
        r#"INSERT INTO daily_events (day, event, target, count) VALUES (?1, ?2, ?3, 1)
           ON CONFLICT (day, event, target) DO UPDATE SET count = count + 1"#,
        day,
        name,
        target,
    )
    .execute(pool)
    .await?;
    Ok(())
}

const TOP: i64 = 10;

/// `cinemap stats` : les `days` derniers jours, aujourd'hui compris.
pub async fn report(pool: &SqlitePool, days: u32) -> anyhow::Result<()> {
    let today = paris_today();
    let since = today
        .checked_sub_days(Days::new(u64::from(days.saturating_sub(1))))
        .unwrap_or(today)
        .format(DATE_FORMAT)
        .to_string();
    println!(
        "Du {since} au {} ({days} jours)\n",
        today.format(DATE_FORMAT)
    );

    let totals = sqlx::query!(
        r#"SELECT event AS "event!: String", sum(count) AS "n!: i64"
           FROM daily_events WHERE day >= ?1 GROUP BY event"#,
        since,
    )
    .fetch_all(pool)
    .await?;
    for event in Event::ALL {
        let n = totals
            .iter()
            .find(|t| t.event == event.name())
            .map_or(0, |t| t.n);
        println!("  {:<26} {n:>7}", event.label());
    }

    for event in [
        Event::Book,
        Event::Add,
        Event::ViewCinema,
        Event::ViewMovie,
        Event::ViewCity,
    ] {
        let name = event.name();
        // Le nom du cinéma ou le titre du film quand il existe encore, l'id sinon.
        let rows = sqlx::query!(
            r#"SELECT coalesce(c.name, m.title, d.target) AS "label!: String",
                      sum(d.count) AS "n!: i64"
               FROM daily_events d
               LEFT JOIN cinemas c ON ?2 <> 'view_movie' AND c.id = d.target
               LEFT JOIN movies m ON ?2 = 'view_movie' AND m.id = CAST(d.target AS INTEGER)
               WHERE d.day >= ?1 AND d.event = ?2
               GROUP BY d.target
               ORDER BY 2 DESC, 1
               LIMIT ?3"#,
            since,
            name,
            TOP,
        )
        .fetch_all(pool)
        .await?;
        if rows.is_empty() {
            continue;
        }
        println!("\n{} : les {TOP} premiers", event.label());
        for row in rows {
            println!("  {:<40} {:>7}", row.label, row.n);
        }
    }

    let per_day = sqlx::query!(
        r#"SELECT day AS "day!: String",
                  coalesce(sum(CASE WHEN event LIKE 'view\_%' ESCAPE '\' THEN count END), 0)
                    AS "views!: i64",
                  coalesce(sum(CASE WHEN event = 'book' THEN count END), 0) AS "book!: i64",
                  coalesce(sum(CASE WHEN event = 'add' THEN count END), 0) AS "add!: i64"
           FROM daily_events WHERE day >= ?1
           GROUP BY day ORDER BY day"#,
        since,
    )
    .fetch_all(pool)
    .await?;
    if !per_day.is_empty() {
        println!("\nPar jour         pages  réserver  ajouts");
        for row in per_day {
            println!(
                "  {}  {:>6}  {:>8}  {:>6}",
                row.day, row.views, row.book, row.add
            );
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_event_parses_back_from_its_name() {
        for event in Event::ALL {
            assert_eq!(Event::parse(event.name()), Some(event));
        }
        assert_eq!(Event::parse("drop table"), None);
    }
}
