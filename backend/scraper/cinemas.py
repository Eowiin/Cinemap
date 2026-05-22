"""
Importe les cinémas d'Île-de-France depuis le dataset open data
data.iledefrance.fr et les stocke dans la base SQLite.

Source : https://data.iledefrance.fr/explore/dataset/les_salles_de_cinemas_en_ile-de-france
310 cinémas, coordonnées GPS incluses, données officielles CNC.

Usage:
    python -m backend.scraper.cinemas
"""

import logging
from typing import Optional
from dataclasses import dataclass

import requests
from tenacity import retry, stop_after_attempt, wait_exponential

from backend.db.models import get_connection, init_db

logging.basicConfig(level=logging.INFO, format="%(asctime)s %(levelname)s %(message)s")
log = logging.getLogger(__name__)

DATASET_URL = (
    "https://data.iledefrance.fr/api/explore/v2.1/catalog/datasets"
    "/les_salles_de_cinemas_en_ile-de-france/exports/json"
)

DEPT_NAMES = {
    "75": "Paris",
    "77": "Seine-et-Marne",
    "78": "Yvelines",
    "91": "Essonne",
    "92": "Hauts-de-Seine",
    "93": "Seine-Saint-Denis",
    "94": "Val-de-Marne",
    "95": "Val-d'Oise",
}


@dataclass
class Cinema:
    source_id: str          # ndeg_auto du dataset
    name: str
    address: Optional[str]
    city: Optional[str]
    zip_code: Optional[str]
    department: Optional[str]
    lat: Optional[float]
    lng: Optional[float]
    screens: Optional[int]
    seats: Optional[int]
    is_art_et_essai: bool
    is_multiplex: bool


@retry(stop=stop_after_attempt(3), wait=wait_exponential(multiplier=1, min=2, max=10))
def fetch_dataset() -> list[dict]:
    log.info("Téléchargement du dataset IDF cinémas…")
    resp = requests.get(DATASET_URL, timeout=30)
    resp.raise_for_status()
    data = resp.json()
    log.info("%d entrées reçues.", len(data))
    return data


def parse_record(record: dict) -> Optional[Cinema]:
    name = record.get("nom", "").strip()
    if not name:
        return None

    geo = record.get("geo") or {}
    lat = geo.get("lat")
    lng = geo.get("lon")

    dept_num = str(int(record["dep"])) if record.get("dep") else None
    zip_code = None
    if dept_num:
        # Code postal : 75 → 75000, 92 → 92000 (approximatif, utilisé pour filtrage)
        zip_code = dept_num.zfill(5) if len(dept_num) == 5 else None

    city = record.get("commune", "").strip() or None
    address = record.get("adresse", "").strip() or None

    # Adresse complète : "146 AVENUE DES CHAMPS ELYSEES, Paris 8e Arrondissement"
    full_address = address
    if city and address and city.lower() not in address.lower():
        full_address = f"{address}, {city}"

    return Cinema(
        source_id=str(int(record["ndeg_auto"])) if record.get("ndeg_auto") else name,
        name=name.title(),
        address=full_address,
        city=city,
        zip_code=dept_num,          # stocke le numéro de département comme proxy
        department=DEPT_NAMES.get(dept_num, dept_num),
        lat=lat,
        lng=lng,
        screens=int(record["ecrans"]) if record.get("ecrans") else None,
        seats=int(record["fauteuils"]) if record.get("fauteuils") else None,
        is_art_et_essai=str(record.get("art_et_essai", "NON")).upper() == "OUI",
        is_multiplex=str(record.get("multiplexe", "NON")).upper() == "OUI",
    )


def upsert_cinema(cinema: Cinema):
    with get_connection() as conn:
        conn.execute(
            """
            INSERT INTO cinemas
                (source_id, name, address, city, zip_code, department,
                 lat, lng, screens, seats, is_art_et_essai, is_multiplex)
            VALUES
                (:source_id, :name, :address, :city, :zip_code, :department,
                 :lat, :lng, :screens, :seats, :is_art_et_essai, :is_multiplex)
            ON CONFLICT(source_id) DO UPDATE SET
                name           = excluded.name,
                address        = excluded.address,
                city           = excluded.city,
                zip_code       = excluded.zip_code,
                department     = excluded.department,
                lat            = excluded.lat,
                lng            = excluded.lng,
                screens        = excluded.screens,
                seats          = excluded.seats,
                is_art_et_essai = excluded.is_art_et_essai,
                is_multiplex   = excluded.is_multiplex,
                scraped_at     = datetime('now')
            """,
            {
                "source_id": cinema.source_id,
                "name": cinema.name,
                "address": cinema.address,
                "city": cinema.city,
                "zip_code": cinema.zip_code,
                "department": cinema.department,
                "lat": cinema.lat,
                "lng": cinema.lng,
                "screens": cinema.screens,
                "seats": cinema.seats,
                "is_art_et_essai": int(cinema.is_art_et_essai),
                "is_multiplex": int(cinema.is_multiplex),
            },
        )


def run():
    init_db()
    records = fetch_dataset()

    ok, skipped = 0, 0
    for record in records:
        cinema = parse_record(record)
        if cinema is None:
            skipped += 1
            continue
        upsert_cinema(cinema)
        coords = f"{cinema.lat:.4f},{cinema.lng:.4f}" if cinema.lat else "sans coords"
        log.info("[OK] %-40s  %s  (%s)", cinema.name, coords, cinema.department)
        ok += 1

    log.info("Terminé : %d cinémas importés, %d ignorés.", ok, skipped)


if __name__ == "__main__":
    run()
