// État de l'application dans l'URL : chaque page est un lien partageable.
// Fonctions pures, le routeur réactif (router.svelte.ts) s'appuie dessus.

import type { VersionFilter } from './api/types';

export type Route =
  | { name: 'home' }
  | { name: 'movie'; id: number }
  | { name: 'cinema'; id: string }
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
  const movie = pathname.match(/^\/film\/(\d+)\/?$/);
  if (movie) return { name: 'movie', id: Number(movie[1]) };
  const cinema = pathname.match(/^\/cinema\/([A-Za-z0-9]+)\/?$/);
  if (cinema) return { name: 'cinema', id: cinema[1]! };
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
      return `/film/${route.id}`;
    case 'cinema':
      return `/cinema/${encodeURIComponent(route.id)}`;
    case 'not-found':
      return '/';
  }
}

export function href(route: Route, filters: UrlFilters): string {
  return routePath(route) + filtersQuery(filters);
}
