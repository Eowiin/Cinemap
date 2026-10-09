// Zone où les séances sont mises à jour chaque nuit (deploy/systemd/cinemap-nightly.service).
// Pour l'instant l'Île-de-France seulement : ailleurs, le front prévient au lieu de laisser
// croire qu'il n'y a pas de séances.

import type { Position } from './api/client';

export const COVERAGE = {
  label: 'Île-de-France',
  departments: ['75', '77', '78', '91', '92', '93', '94', '95'],
  /** Rectangle autour de l'IDF : une position dedans est considérée comme couverte. */
  bbox: { minLat: 48.12, maxLat: 49.24, minLng: 1.44, maxLng: 3.56 },
} as const;

export function isCoveredPosition(p: Position): boolean {
  const { minLat, maxLat, minLng, maxLng } = COVERAGE.bbox;
  return p.lat >= minLat && p.lat <= maxLat && p.lng >= minLng && p.lng <= maxLng;
}

/** Inconnu (`null`) : on ne prévient pas, faute de savoir. */
export function isCoveredPostalCode(postalCode: string | null): boolean {
  return (
    postalCode === null ||
    COVERAGE.departments.some((department) => postalCode.startsWith(department))
  );
}
