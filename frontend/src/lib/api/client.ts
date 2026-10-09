import type {
  Cinema,
  CinemaShowtimes,
  CinemaSummary,
  Meta,
  Movie,
  MovieShowtimes,
  NowShowing,
  SearchResults,
  VersionFilter,
} from './types';

export class ApiError extends Error {
  constructor(
    readonly status: number,
    readonly code: string,
    message: string,
  ) {
    super(message);
  }
}

type Params = Record<string, string | number | boolean | null | undefined>;

/** Construit `path?a=1&b=2` en ignorant les paramètres absents. */
export function apiUrl(path: string, params: Params = {}): string {
  const query = new URLSearchParams();
  for (const [key, value] of Object.entries(params)) {
    if (value !== null && value !== undefined && value !== '') query.set(key, String(value));
  }
  const qs = query.toString();
  return qs ? `${path}?${qs}` : path;
}

async function get<T>(path: string, params: Params = {}, signal?: AbortSignal): Promise<T> {
  let response: Response;
  try {
    response = await fetch(apiUrl(path, params), { signal });
  } catch (error) {
    if (signal?.aborted) throw error;
    throw new ApiError(0, 'network', 'Connexion impossible. Vérifiez votre réseau.');
  }
  if (!response.ok) {
    const body = await response.json().catch(() => null);
    throw new ApiError(
      response.status,
      body?.error?.code ?? 'internal',
      body?.error?.message ?? `Erreur ${response.status}`,
    );
  }
  return response.json() as Promise<T>;
}

export type Position = { lat: number; lng: number };
export type ShowtimeFilters = {
  date?: string;
  version?: VersionFilter | null;
  after?: string | null;
  /** Ids de cartes : cinémas qui acceptent au moins l'une d'elles. */
  cards?: string[] | null;
  /** Ids de cinémas (mes favoris) : seulement ceux-là, où qu'ils soient. */
  cinemas?: string[] | null;
};

/** Liste d'ids en paramètre (`a,b,c`), absente si vide. */
const cardsParam = (ids?: string[] | null) => (ids?.length ? ids.join(',') : undefined);

const position = (p?: Position | null) => (p ? { lat: p.lat, lng: p.lng } : {});

export const api = {
  meta: (signal?: AbortSignal) => get<Meta>('/api/meta', {}, signal),
  cinemas: (signal?: AbortSignal) => get<CinemaSummary[]>('/api/cinemas', {}, signal),
  cinema: (id: string, signal?: AbortSignal) =>
    get<Cinema>(`/api/cinemas/${encodeURIComponent(id)}`, {}, signal),
  cinemaShowtimes: (id: string, filters: ShowtimeFilters, signal?: AbortSignal) =>
    get<CinemaShowtimes>(
      `/api/cinemas/${encodeURIComponent(id)}/showtimes`,
      // `cards` n'a pas de sens pour un seul cinéma : la fiche reste la même.
      { date: filters.date, version: filters.version, after: filters.after },
      signal,
    ),
  nowShowing: (
    filters: ShowtimeFilters & { position?: Position | null; radius_km?: number; limit?: number },
    signal?: AbortSignal,
  ) =>
    get<NowShowing>(
      '/api/movies',
      {
        date: filters.date,
        version: filters.version,
        after: filters.after,
        cards: cardsParam(filters.cards),
        cinemas: cardsParam(filters.cinemas),
        radius_km: filters.position ? filters.radius_km : undefined,
        limit: filters.limit,
        ...position(filters.position),
      },
      signal,
    ),
  movie: (id: number, signal?: AbortSignal) => get<Movie>(`/api/movies/${id}`, {}, signal),
  movieShowtimes: (
    id: number,
    filters: ShowtimeFilters & { position: Position; radius_km?: number },
    signal?: AbortSignal,
  ) =>
    get<MovieShowtimes>(
      `/api/movies/${id}/showtimes`,
      {
        date: filters.date,
        version: filters.version,
        after: filters.after,
        cards: cardsParam(filters.cards),
        cinemas: cardsParam(filters.cinemas),
        radius_km: filters.radius_km,
        ...position(filters.position),
      },
      signal,
    ),
  search: (q: string, signal?: AbortSignal) => get<SearchResults>('/api/search', { q }, signal),
};

/**
 * Lance une requête en annulant la précédente : une réponse lente ne peut plus
 * écraser une réponse plus récente (bug de l'ancien site).
 */
export function latest() {
  let controller: AbortController | null = null;
  return <T>(request: (signal: AbortSignal) => Promise<T>): Promise<T> => {
    controller?.abort();
    controller = new AbortController();
    return request(controller.signal);
  };
}

export const isAbort = (error: unknown) =>
  error instanceof DOMException && error.name === 'AbortError';
