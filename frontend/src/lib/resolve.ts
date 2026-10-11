// Retrouver des séances désignées par un lien (`StopRef`) : via le programme de leur cinéma.

import { api, ApiError } from './api/client';
import { matchesRef, toStop, type PlanStop, type StopRef } from './planner';

/** Un appel par cinéma (2 à 4 en pratique) ; une séance disparue est simplement comptée. */
export async function resolveStops(refs: StopRef[], date: string, signal: AbortSignal) {
  if (!refs.length) return { stops: [] as PlanStop[], missing: 0 };
  const cinemaIds = [...new Set(refs.map((r) => r.cinemaId))];
  const pages = await Promise.all(
    cinemaIds.map((id) =>
      api.cinemaShowtimes(id, { date }, signal).catch((error: unknown) => {
        if (error instanceof ApiError && error.status === 404) return null;
        throw error;
      }),
    ),
  );
  const stops: PlanStop[] = [];
  for (const page of pages) {
    if (!page) continue;
    for (const { movie, showtimes } of page.movies) {
      for (const s of showtimes) {
        const candidate = toStop(page.date, s, movie, page.cinema);
        // Même film à la même heure en VF et en VO (deux salles) : la première suffit.
        const ref = refs.find((r) => matchesRef(r, candidate));
        if (ref && !stops.some((x) => matchesRef(ref, x))) stops.push(candidate);
      }
    }
  }
  return { stops, missing: refs.length - stops.length };
}
