"""
Récupère les codes AlloCiné de tous les cinémas de France et les associe
aux cinémas en base via matching de noms par département.

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

BASE_URL = "https://www.allocine.fr"
HEADERS = {
    "User-Agent": (
        "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) "
        "AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0.0.0 Safari/537.36"
    ),
    "Accept-Language": "fr-FR,fr;q=0.9",
}

# Codes AlloCiné par département (scraping de /salle/ le 2026-05-22)
# Paris n'a pas de page propre — utilise le code IDF agrégé 83093
ALLOCINE_DEPT_CODES: dict[str, str] = {
    "Ain": "83191", "Aisne": "83178", "Allier": "83111",
    "Alpes-de-Haute-Provence": "83185", "Hautes-Alpes": "83186",
    "Alpes-Maritimes": "83187", "Ardèche": "83192", "Ardennes": "83129",
    "Ariège": "83150", "Aube": "83130", "Aude": "83138", "Aveyron": "83151",
    "Bas-Rhin": "83104", "Bouches-du-Rhône": "83188", "Calvados": "83160",
    "Cantal": "83112", "Charente": "83181", "Charente-Maritime": "83182",
    "Cher": "83123", "Corrèze": "83143", "Corse-du-Sud": "265496",
    "Haute-Corse": "83133", "Côte-d'Or": "83115", "Côtes-d'Armor": "83119",
    "Creuse": "83144", "Deux-Sèvres": "83183", "Dordogne": "83106",
    "Doubs": "83134", "Drôme": "83193", "Essonne": "83168", "Eure": "83163",
    "Eure-et-Loir": "83124", "Finistère": "83120", "Gard": "83139",
    "Haute-Garonne": "83152", "Gers": "83153", "Gironde": "83107",
    "Hérault": "83140", "Ille-et-Vilaine": "83121", "Indre": "83125",
    "Indre-et-Loire": "83126", "Isère": "83194", "Jura": "83135",
    "Landes": "83108", "Loir-et-Cher": "83127", "Loire": "83195",
    "Haute-Loire": "83113", "Loire-Atlantique": "83173", "Loiret": "83128",
    "Lot": "83154", "Lot-et-Garonne": "83109", "Lozère": "83141",
    "Maine-et-Loire": "83174", "Manche": "83161", "Marne": "83131",
    "Haute-Marne": "83132", "Mayenne": "83175", "Meurthe-et-Moselle": "83146",
    "Meuse": "83147", "Morbihan": "83122", "Moselle": "83148", "Nièvre": "83116",
    "Nord": "83158", "Oise": "83179", "Orne": "83162", "Pas-de-Calais": "83159",
    "Puy-de-Dôme": "83114", "Pyrénées-Atlantiques": "83110",
    "Hautes-Pyrénées": "83155", "Pyrénées-Orientales": "83142",
    "Haut-Rhin": "83105", "Rhône": "83196", "Haute-Saône": "83136",
    "Saône-et-Loire": "83117", "Sarthe": "83176", "Savoie": "83197",
    "Haute-Savoie": "83198", "Paris": "83093",  # code IDF agrégé (inclut Paris)
    "Seine-Maritime": "83164", "Seine-et-Marne": "83166", "Yvelines": "83167",
    "Somme": "83180", "Tarn": "83156", "Tarn-et-Garonne": "83157",
    "Territoire de Belfort": "83137", "Val-d'Oise": "83172",
    "Val-de-Marne": "83171", "Var": "83189", "Vaucluse": "83190",
    "Vendée": "83177", "Vienne": "83184", "Haute-Vienne": "83145",
    "Vosges": "83149", "Yonne": "83118",
    "Hauts-de-Seine": "83169", "Seine-Saint-Denis": "83170",
    "Guadeloupe": "83199", "Martinique": "83200",
    "Guyane": "83201", "La Réunion": "83202",
}


@retry(stop=stop_after_attempt(3), wait=wait_exponential(min=2, max=8))
def fetch_page(dept_code: str, page: int) -> BeautifulSoup:
    url = f"{BASE_URL}/salle/cinema/departement-{dept_code}/"
    params = {"page": page} if page > 1 else {}
    resp = requests.get(url, params=params, headers=HEADERS, timeout=15)
    resp.raise_for_status()
    return BeautifulSoup(resp.text, "lxml")


def extract_theaters(soup: BeautifulSoup) -> dict[str, str]:
    """Retourne {allocine_id: name} depuis data-theater et data-jan."""
    theaters: dict[str, str] = {}

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
    pages = [a.get("href", "") for a in soup.find_all("a", href=True) if "page=" in a.get("href", "")]
    nums = [int(m.group(1)) for h in pages if (m := re.search(r"page=(\d+)", h))]
    return max(nums) if nums else 1


def scrape_dept_theaters(dept_code: str, dept_name: str) -> dict[str, str]:
    """Scrape toutes les pages d'un département AlloCiné."""
    all_theaters: dict[str, str] = {}
    try:
        first = fetch_page(dept_code, 1)
        all_theaters.update(extract_theaters(first))
        total_pages = max_page(first)

        for page in range(2, total_pages + 1):
            time.sleep(0.8)
            soup = fetch_page(dept_code, page)
            all_theaters.update(extract_theaters(soup))

        log.info("  [%s] %d cinémas AlloCiné", dept_name, len(all_theaters))
    except Exception as exc:
        log.warning("  [%s] Erreur scraping : %s", dept_name, exc)

    return all_theaters


def normalize(text: str) -> str:
    text = unicodedata.normalize("NFD", text)
    text = "".join(c for c in text if unicodedata.category(c) != "Mn")
    text = text.lower()
    text = re.sub(r"[^a-z0-9\s]", " ", text)
    text = re.sub(r"\s+", " ", text).strip()
    for word in ("cinema", "cine", "salle", "le", "la", "les", "de", "du", "des", "l", "d"):
        text = re.sub(rf"\b{word}\b", "", text)
    return re.sub(r"\s+", " ", text).strip()


def best_match(db_name: str, candidates: dict[str, str]) -> tuple[str, str, int] | None:
    target = normalize(db_name)
    if not target:
        return None

    best_id, best_name, best_score = None, None, 0
    for aid, aname in candidates.items():
        candidate = normalize(aname)
        if not candidate:
            continue
        t_words = set(target.split())
        c_words = set(candidate.split())
        if not t_words or not c_words:
            continue
        common = t_words & c_words
        score = len(common) * 2
        if target in candidate or candidate in target:
            score += 3
        if score > best_score:
            best_score = score
            best_id = aid
            best_name = aname

    if best_score >= 2:
        return best_id, best_name, best_score
    return None


def run():
    # Récupère tous les départements présents en base
    with get_connection() as conn:
        depts = [
            r["department"] for r in
            conn.execute(
                "SELECT DISTINCT department FROM cinemas WHERE department IS NOT NULL AND allocine_id IS NULL"
            ).fetchall()
        ]

    log.info("%d départements à traiter.", len(depts))

    total_matched = total_unmatched = 0

    for dept_name in sorted(depts):
        dept_code = ALLOCINE_DEPT_CODES.get(dept_name)
        if not dept_code:
            log.warning("Pas de code AlloCiné pour : %s", dept_name)
            continue

        # Cinémas de ce département sans ID AlloCiné
        with get_connection() as conn:
            cinemas = conn.execute(
                "SELECT id, name, city FROM cinemas WHERE department = ? AND allocine_id IS NULL",
                (dept_name,),
            ).fetchall()

        if not cinemas:
            continue

        theaters = scrape_dept_theaters(dept_code, dept_name)
        time.sleep(1.0)

        matched = unmatched = 0
        with get_connection() as conn:
            for cinema in cinemas:
                result = best_match(cinema["name"], theaters)
                if result:
                    aid, aname, score = result
                    conn.execute(
                        "UPDATE cinemas SET allocine_id = ? WHERE id = ?",
                        (aid, cinema["id"]),
                    )
                    log.info(
                        "    [MATCH s=%d] %-35s -> %s (%s)",
                        score, cinema["name"], aname, aid,
                    )
                    matched += 1
                else:
                    log.debug("    [NO MATCH] %s (%s)", cinema["name"], cinema["city"])
                    unmatched += 1

        log.info("  %s : %d matchés, %d sans correspondance", dept_name, matched, unmatched)
        total_matched += matched
        total_unmatched += unmatched

    log.info("Terminé : %d matchés, %d sans correspondance.", total_matched, total_unmatched)


if __name__ == "__main__":
    run()
