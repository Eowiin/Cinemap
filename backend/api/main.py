import unicodedata
from datetime import date
from typing import Optional

from fastapi import FastAPI, HTTPException, Query
from fastapi.middleware.cors import CORSMiddleware
from fastapi.staticfiles import StaticFiles
from fastapi.responses import FileResponse
from pathlib import Path

from backend.db.models import get_connection

app = FastAPI(title="Cinemap IDF")

app.add_middleware(
    CORSMiddleware,
    allow_origins=["*"],
    allow_methods=["GET"],
    allow_headers=["*"],
)

FRONTEND_DIR = Path(__file__).parent.parent.parent / "frontend"

REGIONS: dict[str, list[str]] = {
    "Île-de-France":            ["Essonne","Hauts-de-Seine","Paris","Seine-et-Marne","Seine-Saint-Denis","Val-de-Marne","Val-d'Oise","Yvelines"],
    "Auvergne-Rhône-Alpes":     ["Ain","Allier","Ardèche","Cantal","Drôme","Haute-Loire","Haute-Savoie","Isère","Loire","Puy-de-Dôme","Rhône","Savoie"],
    "Bourgogne-Franche-Comté":  ["Côte-d'Or","Doubs","Jura","Nièvre","Haute-Saône","Saône-et-Loire","Territoire de Belfort","Yonne"],
    "Bretagne":                 ["Côtes-d'Armor","Finistère","Ille-et-Vilaine","Morbihan"],
    "Centre-Val de Loire":      ["Cher","Eure-et-Loir","Indre","Indre-et-Loire","Loir-et-Cher","Loiret"],
    "Corse":                    ["Corse-du-Sud","Haute-Corse"],
    "Grand Est":                ["Ardennes","Aube","Bas-Rhin","Haut-Rhin","Haute-Marne","Marne","Meurthe-et-Moselle","Meuse","Moselle","Vosges"],
    "Hauts-de-France":          ["Aisne","Nord","Oise","Pas-de-Calais","Somme"],
    "Normandie":                ["Calvados","Eure","Manche","Orne","Seine-Maritime"],
    "Nouvelle-Aquitaine":       ["Charente","Charente-Maritime","Corrèze","Creuse","Deux-Sèvres","Dordogne","Gironde","Landes","Lot-et-Garonne","Pyrénées-Atlantiques","Vienne","Haute-Vienne"],
    "Occitanie":                ["Ariège","Aude","Aveyron","Gard","Gers","Haute-Garonne","Hautes-Pyrénées","Hérault","Lot","Lozère","Pyrénées-Orientales","Tarn","Tarn-et-Garonne"],
    "Pays de la Loire":         ["Loire-Atlantique","Maine-et-Loire","Mayenne","Sarthe","Vendée"],
    "Provence-Alpes-Côte d'Azur": ["Alpes-de-Haute-Provence","Alpes-Maritimes","Bouches-du-Rhône","Hautes-Alpes","Var","Vaucluse"],
    "Outre-mer":                ["Guadeloupe","Guyane","La Réunion","Martinique","Mayotte"],
}


@app.get("/api/cinemas")
def get_cinemas(
    dept: Optional[str] = None,
    region: Optional[str] = None,
    art_et_essai: Optional[bool] = None,
    vo: Optional[bool] = None,
    show_date: Optional[str] = None,
):
    alias = "c" if vo else ""
    dot = "c." if vo else ""

    if vo:
        if show_date is None:
            show_date = date.today().isoformat()
        query = """
            SELECT DISTINCT c.* FROM cinemas c
            JOIN showtimes s ON s.cinema_id = c.id
            WHERE c.lat IS NOT NULL AND c.lng IS NOT NULL
            AND s.version = 'VO' AND s.date = ?
        """
        params: list = [show_date]
    else:
        query = "SELECT * FROM cinemas WHERE lat IS NOT NULL AND lng IS NOT NULL"
        params = []

    if dept:
        query += f" AND {dot}department = ?"
        params.append(dept)
    elif region:
        # Normalisation NFC pour éviter les écarts Unicode (ex: Île-de-France)
        nfc = lambda s: unicodedata.normalize("NFC", s)
        region_key = next((k for k in REGIONS if nfc(k) == nfc(region)), None)
        if region_key:
            depts = REGIONS[region_key]
            placeholders = ",".join("?" * len(depts))
            query += f" AND {dot}department IN ({placeholders})"
            params.extend(depts)

    if art_et_essai is not None:
        query += f" AND {dot}is_art_et_essai = ?"
        params.append(int(art_et_essai))

    query += f" ORDER BY {dot}name"

    with get_connection() as conn:
        rows = conn.execute(query, params).fetchall()

    return [dict(r) for r in rows]


@app.get("/api/cinemas/{cinema_id}/showtimes")
def get_showtimes(cinema_id: int, show_date: str = Query(default=None)):
    if show_date is None:
        show_date = date.today().isoformat()

    with get_connection() as conn:
        cinema = conn.execute(
            "SELECT * FROM cinemas WHERE id = ?", (cinema_id,)
        ).fetchone()
        if not cinema:
            raise HTTPException(status_code=404, detail="Cinéma introuvable")

        rows = conn.execute(
            """
            SELECT movie_title, movie_allocine_id, poster_url,
                   starts_at, version, experience
            FROM showtimes
            WHERE cinema_id = ? AND date = ?
            ORDER BY movie_title, starts_at
            """,
            (cinema_id, show_date),
        ).fetchall()

    # Regroupe les séances par film
    movies: dict[str, dict] = {}
    for r in rows:
        title = r["movie_title"]
        if title not in movies:
            movies[title] = {
                "title": title,
                "allocine_id": r["movie_allocine_id"],
                "poster_url": r["poster_url"],
                "showtimes": [],
            }
        movies[title]["showtimes"].append({
            "starts_at": r["starts_at"],
            "version": r["version"],
            "experience": r["experience"],
        })

    return {
        "cinema": dict(cinema),
        "date": show_date,
        "movies": list(movies.values()),
    }


@app.get("/api/search")
def search(q: str, show_date: str = Query(default=None)):
    if not q or len(q) < 2:
        return {"cinemas": [], "movies": []}
    if show_date is None:
        show_date = date.today().isoformat()

    like = f"%{q}%"
    with get_connection() as conn:
        cinemas = conn.execute(
            """SELECT id, name, city, department, lat, lng, is_art_et_essai
               FROM cinemas WHERE name LIKE ? AND lat IS NOT NULL LIMIT 8""",
            (like,),
        ).fetchall()

        # Groupe d'abord par film, LIMIT sur les films distincts
        movie_rows = conn.execute(
            """SELECT s.movie_title, s.poster_url,
                      GROUP_CONCAT(DISTINCT c.id) AS cinema_ids,
                      COUNT(DISTINCT c.id)        AS cinema_count
               FROM showtimes s JOIN cinemas c ON c.id = s.cinema_id
               WHERE s.movie_title LIKE ? AND s.date = ?
               GROUP BY s.movie_title
               ORDER BY cinema_count DESC
               LIMIT 8""",
            (like, show_date),
        ).fetchall()

    movies_out = []
    for r in movie_rows:
        ids = [int(i) for i in r["cinema_ids"].split(",")]
        movies_out.append({
            "title":      r["movie_title"],
            "poster_url": r["poster_url"],
            "cinemas":    [{"id": i} for i in ids],
        })

    return {
        "cinemas": [dict(c) for c in cinemas],
        "movies":  movies_out,
    }


@app.get("/api/today")
def today_overview(show_date: str = Query(default=None)):
    if show_date is None:
        show_date = date.today().isoformat()
    with get_connection() as conn:
        stats = conn.execute(
            """SELECT
                 COUNT(DISTINCT cinema_id)  AS cinemas_with_showtimes,
                 COUNT(DISTINCT movie_title) AS movie_count,
                 COUNT(*)                   AS showtime_count
               FROM showtimes WHERE date = ?""",
            (show_date,),
        ).fetchone()

        top_movies = conn.execute(
            """SELECT movie_title, poster_url,
                      COUNT(DISTINCT cinema_id) AS cinema_count,
                      GROUP_CONCAT(DISTINCT cinema_id) AS cinema_ids
               FROM showtimes
               WHERE date = ?
               GROUP BY movie_title
               ORDER BY cinema_count DESC
               LIMIT 10""",
            (show_date,),
        ).fetchall()

    return {
        "date": show_date,
        "stats": dict(stats),
        "top_movies": [dict(r) for r in top_movies],
    }


@app.get("/api/departments")
def get_departments():
    with get_connection() as conn:
        rows = conn.execute(
            "SELECT DISTINCT department FROM cinemas WHERE department IS NOT NULL ORDER BY department"
        ).fetchall()
    return [r["department"] for r in rows]


@app.get("/", response_class=FileResponse)
def index():
    return FRONTEND_DIR / "index.html"


class NoCacheStaticFiles(StaticFiles):
    async def get_response(self, path: str, scope):
        response = await super().get_response(path, scope)
        response.headers["Cache-Control"] = "no-store"
        return response

app.mount("/", NoCacheStaticFiles(directory=str(FRONTEND_DIR)), name="static")
