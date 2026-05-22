"""
Scrape les séances AlloCiné pour tous les cinémas IDF ayant un allocine_id
et les stocke dans la table showtimes de la base SQLite.

Usage:
    python -m backend.scraper.showtimes [--date YYYY-MM-DD]
"""

import argparse
import logging
import time
from datetime import date, timedelta

import requests
from tenacity import retry, stop_after_attempt, wait_exponential

from backend.db.models import get_connection, init_db

logging.basicConfig(level=logging.INFO, format="%(asctime)s %(levelname)s %(message)s")
log = logging.getLogger(__name__)

SHOWTIMES_URL = "https://www.allocine.fr/_/showtimes/theater-{theater_id}/d-{date}/"
HEADERS = {
    "User-Agent": (
        "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) "
        "AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0.0.0 Safari/537.36"
    ),
    "Accept": "application/json",
    "Accept-Language": "fr-FR,fr;q=0.9",
    "Referer": "https://www.allocine.fr/",
}

# Type de séance → label lisible
VERSION_LABELS = {
    "ORIGINAL": "VO",
    "DUBBED": "VF",
    "LOCAL": "VF",
}


@retry(stop=stop_after_attempt(3), wait=wait_exponential(min=2, max=10))
def fetch_showtimes(theater_id: str, show_date: str) -> dict:
    url = SHOWTIMES_URL.format(theater_id=theater_id, date=show_date)
    resp = requests.get(url, headers=HEADERS, timeout=15)
    resp.raise_for_status()
    return resp.json()


def parse_and_store(cinema_id: int, theater_id: str, show_date: str):
    try:
        data = fetch_showtimes(theater_id, show_date)
    except Exception as exc:
        log.warning("Erreur pour %s le %s : %s", theater_id, show_date, exc)
        return 0

    if data.get("error"):
        log.warning("API error pour %s : %s", theater_id, data.get("message"))
        return 0

    count = 0
    with get_connection() as conn:
        # Supprime les séances existantes pour ce cinéma/date avant de réinsérer
        conn.execute(
            "DELETE FROM showtimes WHERE cinema_id = ? AND date = ?",
            (cinema_id, show_date),
        )

        for result in data.get("results", []):
            movie = result.get("movie") or {}
            movie_title = movie.get("title", "")
            movie_id = str(movie.get("internalId", ""))
            poster_url = (movie.get("poster") or {}).get("url")

            for version_type, sessions in result.get("showtimes", {}).items():
                for session in sessions:
                    starts_at = session.get("startsAt")
                    if not starts_at:
                        continue

                    version = VERSION_LABELS.get(
                        session.get("diffusionVersion", ""), "VF"
                    )
                    exp = session.get("experience") or ""
                    experience = ", ".join(exp) if isinstance(exp, list) else str(exp)
                    showtime_id = str(session.get("internalId", ""))

                    conn.execute(
                        """
                        INSERT OR REPLACE INTO showtimes
                            (cinema_id, allocine_showtime_id, movie_title,
                             movie_allocine_id, poster_url, starts_at,
                             date, version, experience)
                        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
                        """,
                        (
                            cinema_id,
                            showtime_id,
                            movie_title,
                            movie_id,
                            poster_url,
                            starts_at,
                            show_date,
                            version,
                            experience,
                        ),
                    )
                    count += 1

    return count


def purge_old(keep_days: int = 2):
    cutoff = (date.today() - timedelta(days=keep_days)).isoformat()
    with get_connection() as conn:
        deleted = conn.execute(
            "DELETE FROM showtimes WHERE date < ?", (cutoff,)
        ).rowcount
    log.info("Purge : %d séances supprimées (antérieures au %s)", deleted, cutoff)


def run(target_date: str | None = None, days: int = 1):
    init_db()
    purge_old()

    if target_date is None:
        target_date = date.today().isoformat()

    with get_connection() as conn:
        cinemas = conn.execute(
            "SELECT id, name, allocine_id FROM cinemas WHERE allocine_id IS NOT NULL"
        ).fetchall()

    log.info(
        "%d cinémas avec ID AlloCiné — scraping des séances du %s (%d jour(s))",
        len(cinemas), target_date, days,
    )

    total = 0
    for cinema in cinemas:
        for offset in range(days):
            d = (date.fromisoformat(target_date) + timedelta(days=offset)).isoformat()
            count = parse_and_store(cinema["id"], cinema["allocine_id"], d)
            log.info(
                "  [%s] %s — %d séances le %s",
                cinema["allocine_id"], cinema["name"], count, d,
            )
            time.sleep(1.5)

        total += 1

    log.info("Terminé : %d cinémas traités.", total)


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--date", help="Date YYYY-MM-DD (défaut: aujourd'hui)")
    parser.add_argument("--days", type=int, default=1, help="Nombre de jours à scraper")
    args = parser.parse_args()
    run(target_date=args.date, days=args.days)
