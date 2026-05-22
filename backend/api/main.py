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


@app.get("/api/cinemas")
def get_cinemas(dept: Optional[str] = None, art_et_essai: Optional[bool] = None):
    query = "SELECT * FROM cinemas WHERE lat IS NOT NULL AND lng IS NOT NULL"
    params: list = []
    if dept:
        query += " AND department = ?"
        params.append(dept)
    if art_et_essai is not None:
        query += " AND is_art_et_essai = ?"
        params.append(int(art_et_essai))
    query += " ORDER BY name"

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
