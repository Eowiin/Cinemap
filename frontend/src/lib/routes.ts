// État de l'application dans l'URL : chaque page est un lien partageable.
// Fonctions pures, le routeur réactif (router.svelte.ts) s'appuie dessus.

import type { VersionFilter } from './api/types';
import { cinemaSlug, slugify } from './seo';

export type Route =
  | { name: 'home' }
  /** `slug` : le nom lisible après l'id (`/film/42-garance`), sans effet sur la page. */
  | { name: 'movie'; id: number; slug?: string }
  | { name: 'cinema'; id: string; slug?: string }
  /** Même écran que l'accueil, centré sur la ville. */
  | { name: 'city'; slug: string }
  | { name: 'programme' }
  | { name: 'about' }
  | { name: 'not-found' };

export type UrlFilters = {
  date: string | null;
  version: VersionFilter | null;
  after: string | null;
  /** Ids de cartes d'abonnement (`cards=ugc_illimite,pathe_cinepass`), null = toutes. */
  cards: string[] | null;
  /** `favoris=1` : seulement mes cinémas favoris (gardés dans le navigateur). */
  favorites: boolean;
};

export function parseRoute(pathname: string): Route {
  if (pathname === '/' || pathname === '') return { name: 'home' };
  const movie = pathname.match(/^\/film\/(\d+)(?:-([^/]*))?\/?$/);
  if (movie) return { name: 'movie', id: Number(movie[1]), slug: movie[2] || undefined };
  const cinema = pathname.match(/^\/cinema\/([A-Za-z0-9]+)(?:-([^/]*))?\/?$/);
  if (cinema) return { name: 'cinema', id: cinema[1]!, slug: cinema[2] || undefined };
  const city = pathname.match(/^\/ville\/([a-z0-9-]+)\/?$/);
  if (city) return { name: 'city', slug: city[1]! };
  if (/^\/programme\/?$/.test(pathname)) return { name: 'programme' };
  if (/^\/a-propos\/?$/.test(pathname)) return { name: 'about' };
  return { name: 'not-found' };
}

/** Lit les filtres en ignorant les valeurs invalides (un lien abîmé ne doit pas casser la page). */
export function parseFilters(search: string): UrlFilters {
  const params = new URLSearchParams(search);
  const date = params.get('date');
  const version = params.get('version');
  const after = params.get('after');
  // Ids bien formés seulement ; un id inconnu du serveur donnera un 400 affiché par la page.
  const cards = [
    ...new Set((params.get('cards') ?? '').split(',').filter((id) => /^[a-z0-9_]+$/.test(id))),
  ];
  return {
    date: date && /^\d{4}-\d{2}-\d{2}$/.test(date) ? date : null,
    version: version === 'VF' || version === 'VO' ? version : null,
    after: after && /^([01]\d|2[0-3]):[0-5]\d$/.test(after) ? after : null,
    cards: cards.length ? cards : null,
    favorites: params.get('favoris') === '1',
  };
}

export function filtersQuery(filters: UrlFilters): string {
  const params = new URLSearchParams();
  if (filters.date) params.set('date', filters.date);
  if (filters.version) params.set('version', filters.version);
  if (filters.after) params.set('after', filters.after);
  if (filters.cards?.length) params.set('cards', filters.cards.join(','));
  if (filters.favorites) params.set('favoris', '1');
  const qs = params.toString();
  return qs ? `?${qs}` : '';
}

export function routePath(route: Route): string {
  switch (route.name) {
    case 'home':
      return '/';
    case 'movie':
      return withSlug(`/film/${route.id}`, route.slug);
    case 'cinema':
      return withSlug(`/cinema/${encodeURIComponent(route.id)}`, route.slug);
    case 'city':
      return `/ville/${route.slug}`;
    case 'programme':
      return '/programme';
    case 'about':
      return '/a-propos';
    case 'not-found':
      return '/';
  }
}

function withSlug(path: string, slug: string | undefined): string {
  return slug ? `${path}-${encodeURIComponent(slug)}` : path;
}

export function movieRoute(movie: { id: number; title: string }): Route {
  return { name: 'movie', id: movie.id, slug: slugify(movie.title) || undefined };
}

/** `city` absent (cinéma gardé dans le programme) : le serveur redirige vers l'URL complète. */
export function cinemaRoute(cinema: { id: string; name: string; city?: string | null }): Route {
  return {
    name: 'cinema',
    id: cinema.id,
    slug: cinemaSlug(cinema.name, cinema.city ?? null) || undefined,
  };
}

export function href(route: Route, filters: UrlFilters): string {
  return routePath(route) + filtersQuery(filters);
}
