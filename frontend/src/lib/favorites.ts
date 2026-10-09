// Cinémas favoris : ce qu'on garde de chacun pour l'afficher sans l'API.
// (Les films favoris ont été retirés le 2026-10-09 : pas d'usage clair.)

import type { CinemaSummary } from './api/types';
import { isRecord } from './stored';

export type FavoriteCinema = { id: string; name: string; city: string | null };

export const favoriteCinema = (c: CinemaSummary): FavoriteCinema => ({
  id: c.id,
  name: c.name,
  city: c.city,
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
