// Favoris (cinémas, films) : ce qu'on garde de chacun pour l'afficher sans l'API.

import type { CinemaSummary, MovieSummary } from './api/types';
import { isRecord } from './stored';

export type FavoriteCinema = { id: string; name: string; city: string | null };
export type FavoriteMovie = { id: number; title: string; poster_url: string | null };

export const favoriteCinema = (c: CinemaSummary): FavoriteCinema => ({
  id: c.id,
  name: c.name,
  city: c.city,
});

export const favoriteMovie = (m: MovieSummary): FavoriteMovie => ({
  id: m.id,
  title: m.title,
  poster_url: m.poster_url,
});

const isNullableString = (value: unknown) => value === null || typeof value === 'string';

export function isFavoriteCinema(value: unknown): value is FavoriteCinema {
  return (
    isRecord(value) &&
    typeof value.id === 'string' &&
    typeof value.name === 'string' &&
    isNullableString(value.city)
  );
}

export function isFavoriteMovie(value: unknown): value is FavoriteMovie {
  return (
    isRecord(value) &&
    Number.isInteger(value.id) &&
    typeof value.title === 'string' &&
    isNullableString(value.poster_url)
  );
}
