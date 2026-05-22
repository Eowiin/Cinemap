"""
Récupère les codes AlloCiné de tous les cinémas IDF et les associe
aux cinémas déjà en base via matching de noms.

Usage:
    python -m backend.scraper.allocine_match
"""

import json
import logging
import re
import time
import unicodedata

import requests
from bs4 import BeautifulSoup
from tenacity import retry, stop_after_attempt, wait_exponential

from backend.db.models import get_connection

logging.basicConfig(level=logging.INFO, format="%(asctime)s %(levelname)s %(message)s")
log = logging.getLogger(__name__)

# AlloCiné regroupe toute l'IDF sous un seul identifiant de région
ALLOCINE_IDF_DEPT = "83093"
BASE_URL = "https://www.allocine.fr"
HEADERS = {
    "User-Agent": (
        "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) "
        "AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0.0.0 Safari/537.36"
    ),
    "Accept-Language": "fr-FR,fr;q=0.9",
}


@retry(stop=stop_after_attempt(3), wait=wait_exponential(min=2, max=8))
def fetch_page(dept: str, page: int) -> BeautifulSoup:
    url = f"{BASE_URL}/salle/cinema/departement-{dept}/"
    params = {"page": page} if page > 1 else {}
    resp = requests.get(url, params=params, headers=HEADERS, timeout=15)
    resp.raise_for_status()
    return BeautifulSoup(resp.text, "lxml")


def extract_theaters(soup: BeautifulSoup) -> dict[str, str]:
    """Retourne {allocine_id: name} depuis data-theater et data-jan."""
    theaters = {}

    for el in soup.find_all(attrs={"data-theater": True}):
        try:
            d = json.loads(el["data-theater"])
            theaters[d["id"]] = d.get("name", "")
        except Exception:
            pass

    for el in soup.find_all(attrs={"data-jan": True}):
        try:
            d = json.loads(el["data-jan"])
            te = d.get("theater_entity", "")
            m = re.match(r"([A-Z][0-9A-Z]+)\|(.+)", te)
            if m and m.group(1) not in theaters:
                theaters[m.group(1)] = m.group(2).replace("_", " ").title()
        except Exception:
            pass

    return theaters


def max_page(soup: BeautifulSoup) -> int:
    pages = [
        a.get("href", "")
        for a in soup.find_all("a", href=True)
        if "page=" in a.get("href", "")
    ]
    nums = [int(m.group(1)) for h in pages if (m := re.search(r"page=(\d+)", h))]
    return max(nums) if nums else 1


def scrape_all_allocine_theaters() -> dict[str, str]:
    """Scrape toutes les pages IDF et retourne {allocine_id: name}."""
    all_theaters: dict[str, str] = {}

    first = fetch_page(ALLOCINE_IDF_DEPT, 1)
    all_theaters.update(extract_theaters(first))
    total_pages = max_page(first)
    log.info("AlloCiné IDF : %d pages à scraper", total_pages)

    for page in range(2, total_pages + 1):
        time.sleep(0.8)
        soup = fetch_page(ALLOCINE_IDF_DEPT, page)
        batch = extract_theaters(soup)
        all_theaters.update(batch)
        log.info("  Page %d/%d — %d cinémas cumulés", page, total_pages, len(all_theaters))

    return all_theaters


def normalize(text: str) -> str:
    """Normalise un nom pour comparaison : minuscules, sans accents ni ponctuation."""
    text = unicodedata.normalize("NFD", text)
    text = "".join(c for c in text if unicodedata.category(c) != "Mn")
    text = text.lower()
    text = re.sub(r"[^a-z0-9\s]", " ", text)
    text = re.sub(r"\s+", " ", text).strip()
    # Supprime les mots génériques pour améliorer le matching
    for word in ("cinema", "cine", "salle", "le", "la", "les", "de", "du", "des", "l", "d"):
        text = re.sub(rf"\b{word}\b", "", text)
    return re.sub(r"\s+", " ", text).strip()


def best_match(
    db_name: str, candidates: dict[str, str]
) -> tuple[str, str, int] | None:
    """
    Cherche le meilleur candidat AlloCiné pour un nom de cinéma.
    Retourne (allocine_id, matched_name, score) ou None si aucun match satisfaisant.
    """
    target = normalize(db_name)
    if not target:
        return None

    best_id, best_name, best_score = None, None, 0

    for aid, aname in candidates.items():
        candidate = normalize(aname)
        if not candidate:
            continue

        # Score basé sur les tokens communs
        t_words = set(target.split())
        c_words = set(candidate.split())
        if not t_words or not c_words:
            continue

        common = t_words & c_words
        score = len(common) * 2
        # Bonus si un est contenu dans l'autre
        if target in candidate or candidate in target:
            score += 3

        if score > best_score:
            best_score = score
            best_id = aid
            best_name = aname

    # Seuil minimum : au moins 2 tokens communs
    if best_score >= 2:
        return best_id, best_name, best_score
    return None


def run():
    log.info("Scraping de la liste AlloCiné IDF…")
    allocine_theaters = scrape_all_allocine_theaters()
    log.info("Total AlloCiné IDF : %d cinémas", len(allocine_theaters))

    with get_connection() as conn:
        cinemas = conn.execute(
            "SELECT id, name, city FROM cinemas WHERE allocine_id IS NULL"
        ).fetchall()

    log.info("Cinémas en base sans ID AlloCiné : %d", len(cinemas))

    matched, unmatched = 0, 0
    with get_connection() as conn:
        for cinema in cinemas:
            result = best_match(cinema["name"], allocine_theaters)
            if result:
                aid, aname, score = result
                conn.execute(
                    "UPDATE cinemas SET allocine_id = ? WHERE id = ?",
                    (aid, cinema["id"]),
                )
                log.info(
                    "  [MATCH s=%d] %-35s -> %s (%s)",
                    score, cinema["name"], aname, aid,
                )
                matched += 1
            else:
                log.warning("  [NO MATCH] %s (%s)", cinema["name"], cinema["city"])
                unmatched += 1

    log.info("Résultat : %d matchés, %d sans correspondance.", matched, unmatched)


if __name__ == "__main__":
    run()
