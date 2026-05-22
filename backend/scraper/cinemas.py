"""
Importe les cinémas de France depuis le dataset OpenStreetMap (data.gouv.fr)
et les stocke dans la base SQLite. Croise avec le dataset CNC pour l'Art & Essai.

Sources :
  - OSM : https://www.data.gouv.fr/datasets/cinemas-issus-dopenstreetmap (GPS, nom, commune)
  - CNC : https://www.data.gouv.fr/datasets/liste-des-etablissements-cinematographiques-actifs-1
          (Art & Essai, salles, fauteuils — feuille 2025)

Usage:
    python -m backend.scraper.cinemas [--reset]
"""

import argparse
import csv
import io
import logging
import zipfile

import openpyxl
import requests
from tenacity import retry, stop_after_attempt, wait_exponential

from backend.db.models import get_connection, init_db

logging.basicConfig(level=logging.INFO, format="%(asctime)s %(levelname)s %(message)s")
log = logging.getLogger(__name__)

OSM_CSV_URL = "https://geodatamine.fr/dump/cinema_csv.zip"
CNC_XLSX_URL = "https://www.data.gouv.fr/api/1/datasets/r/cdb918e7-7f1a-44fc-bf6f-c59d1614ed6d"

DATAGOUV_DATASET = "cinemas-issus-dopenstreetmap"

DEPT_NAMES = {
    "01": "Ain", "02": "Aisne", "03": "Allier", "04": "Alpes-de-Haute-Provence",
    "05": "Hautes-Alpes", "06": "Alpes-Maritimes", "07": "Ardèche", "08": "Ardennes",
    "09": "Ariège", "10": "Aube", "11": "Aude", "12": "Aveyron",
    "13": "Bouches-du-Rhône", "14": "Calvados", "15": "Cantal", "16": "Charente",
    "17": "Charente-Maritime", "18": "Cher", "19": "Corrèze",
    "2A": "Corse-du-Sud", "2B": "Haute-Corse",
    "21": "Côte-d'Or", "22": "Côtes-d'Armor", "23": "Creuse",
    "24": "Dordogne", "25": "Doubs", "26": "Drôme",
    "27": "Eure", "28": "Eure-et-Loir", "29": "Finistère",
    "30": "Gard", "31": "Haute-Garonne", "32": "Gers", "33": "Gironde",
    "34": "Hérault", "35": "Ille-et-Vilaine", "36": "Indre", "37": "Indre-et-Loire",
    "38": "Isère", "39": "Jura", "40": "Landes",
    "41": "Loir-et-Cher", "42": "Loire", "43": "Haute-Loire", "44": "Loire-Atlantique",
    "45": "Loiret", "46": "Lot", "47": "Lot-et-Garonne", "48": "Lozère",
    "49": "Maine-et-Loire", "50": "Manche", "51": "Marne", "52": "Haute-Marne",
    "53": "Mayenne", "54": "Meurthe-et-Moselle", "55": "Meuse", "56": "Morbihan",
    "57": "Moselle", "58": "Nièvre", "59": "Nord",
    "60": "Oise", "61": "Orne", "62": "Pas-de-Calais", "63": "Puy-de-Dôme",
    "64": "Pyrénées-Atlantiques", "65": "Hautes-Pyrénées", "66": "Pyrénées-Orientales",
    "67": "Bas-Rhin", "68": "Haut-Rhin", "69": "Rhône",
    "70": "Haute-Saône", "71": "Saône-et-Loire", "72": "Sarthe", "73": "Savoie",
    "74": "Haute-Savoie", "75": "Paris", "76": "Seine-Maritime",
    "77": "Seine-et-Marne", "78": "Yvelines", "79": "Deux-Sèvres",
    "80": "Somme", "81": "Tarn", "82": "Tarn-et-Garonne", "83": "Var",
    "84": "Vaucluse", "85": "Vendée", "86": "Vienne", "87": "Haute-Vienne",
    "88": "Vosges", "89": "Yonne", "90": "Territoire de Belfort",
    "91": "Essonne", "92": "Hauts-de-Seine", "93": "Seine-Saint-Denis",
    "94": "Val-de-Marne", "95": "Val-d'Oise",
    "971": "Guadeloupe", "972": "Martinique", "973": "Guyane",
    "974": "La Réunion", "976": "Mayotte",
}


def dept_from_insee(com_insee: str) -> tuple[str, str]:
    """Retourne (dept_num, dept_name) depuis le code INSEE commune."""
    s = str(com_insee).strip()
    # Corse avec codes alphabétiques
    if s.upper().startswith("2A"):
        return "2A", "Corse-du-Sud"
    if s.upper().startswith("2B"):
        return "2B", "Haute-Corse"
    try:
        padded = s.zfill(5)
        if padded.startswith("97"):
            num = padded[:3]
        elif padded[:2] == "20":
            # Corse stockée en numérique : 20xxx
            num = "2A" if int(padded) < 20200 else "2B"
        else:
            num = str(int(padded[:2])).zfill(2)
        return num, DEPT_NAMES.get(num, num)
    except (ValueError, IndexError):
        return s[:2], s[:2]


@retry(stop=stop_after_attempt(3), wait=wait_exponential(min=2, max=10))
def fetch_osm_csv() -> list[dict]:
    """Télécharge et parse le CSV OSM depuis data.gouv.fr."""
    log.info("Téléchargement du dataset OSM cinémas…")
    # Récupère l'URL courante depuis l'API data.gouv.fr
    meta = requests.get(
        f"https://www.data.gouv.fr/api/1/datasets/{DATAGOUV_DATASET}/",
        timeout=15,
    ).json()
    csv_url = next(
        (r["url"] for r in meta.get("resources", []) if "csv" in r.get("title", "").lower()),
        OSM_CSV_URL,
    )
    resp = requests.get(csv_url, timeout=60)
    resp.raise_for_status()

    with zipfile.ZipFile(io.BytesIO(resp.content)) as z:
        csv_name = next(n for n in z.namelist() if n.endswith(".csv") and "metadata" not in n)
        with z.open(csv_name) as f:
            rows = list(csv.DictReader(io.TextIOWrapper(f, encoding="utf-8"), delimiter=";"))

    log.info("%d entrées OSM reçues.", len(rows))
    return rows


@retry(stop=stop_after_attempt(3), wait=wait_exponential(min=2, max=10))
def fetch_cnc_ae_map() -> dict[str, bool]:
    """Télécharge le XLSX CNC et retourne {cnc_id: is_art_et_essai} depuis la feuille 2025."""
    log.info("Téléchargement du dataset CNC (Art & Essai)…")
    resp = requests.get(CNC_XLSX_URL, timeout=60)
    resp.raise_for_status()

    wb = openpyxl.load_workbook(io.BytesIO(resp.content))
    # Utilise la feuille la plus récente disponible
    year_sheets = [s for s in wb.sheetnames if s.isdigit()]
    sheet_name = max(year_sheets, key=int)
    log.info("Feuille CNC utilisée : %s", sheet_name)
    ws = wb[sheet_name]

    ae_map: dict[str, bool] = {}
    for row in ws.iter_rows(min_row=6, values_only=True):  # données à partir de la ligne 6
        cnc_id = row[0]
        ae_val = row[7]
        if cnc_id is not None:
            try:
                ae_map[str(int(cnc_id))] = str(ae_val).strip().upper() == "OUI"
            except (ValueError, TypeError):
                pass

    log.info("%d entrées CNC chargées.", len(ae_map))
    return ae_map


def run(reset: bool = False):
    init_db()

    if reset:
        log.info("Reset : suppression des données existantes…")
        with get_connection() as conn:
            conn.execute("DELETE FROM showtimes")
            conn.execute("DELETE FROM cinemas")

    osm_rows = fetch_osm_csv()
    cnc_ae = fetch_cnc_ae_map()

    ok = skipped = 0
    for row in osm_rows:
        name = (row.get("name") or "").strip()
        if not name:
            skipped += 1
            continue

        # Coordonnées GPS
        try:
            lng = float(row["X"])
            lat = float(row["Y"])
        except (ValueError, TypeError):
            skipped += 1
            continue

        # Département
        dept_num, dept_name = dept_from_insee(row.get("com_insee", ""))

        # Art & Essai depuis CNC
        ref_cnc = str(row.get("ref_cnc") or "").strip()
        is_ae = cnc_ae.get(ref_cnc, False)

        # Salles et fauteuils depuis OSM (complété par CNC si disponible)
        try:
            screens = int(row["nb_screens"]) if row.get("nb_screens") else None
        except ValueError:
            screens = None
        try:
            seats = int(row["capacity"]) if row.get("capacity") else None
        except ValueError:
            seats = None

        source_id = row.get("osm_id") or name

        with get_connection() as conn:
            conn.execute(
                """
                INSERT INTO cinemas
                    (source_id, name, city, department,
                     lat, lng, screens, seats, is_art_et_essai)
                VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
                ON CONFLICT(source_id) DO UPDATE SET
                    name           = excluded.name,
                    city           = excluded.city,
                    department     = excluded.department,
                    lat            = excluded.lat,
                    lng            = excluded.lng,
                    screens        = excluded.screens,
                    seats          = excluded.seats,
                    is_art_et_essai = excluded.is_art_et_essai,
                    scraped_at     = datetime('now')
                """,
                (
                    source_id, name,
                    row.get("com_nom") or None,
                    dept_name,
                    lat, lng, screens, seats,
                    int(is_ae),
                ),
            )
        ok += 1

    log.info("Terminé : %d cinémas importés, %d ignorés.", ok, skipped)


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--reset", action="store_true",
        help="Supprime toutes les données existantes avant d'importer"
    )
    args = parser.parse_args()
    run(reset=args.reset)
