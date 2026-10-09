// Cartes d'abonnement : filtre de la barre de filtres (mémorisé dans le navigateur) et badges.

import type { Card } from './api/types';
import { isRecord } from './stored';

/** Une carte retenue dans le filtre : son id suffit, le nom vient de `/api/meta`. */
export type CardRef = { id: string };

export function isCardRef(value: unknown): value is CardRef {
  return isRecord(value) && typeof value.id === 'string';
}

/** Ajoute ou retire une carte du filtre ; `null` (pas de filtre) quand il n'en reste aucune. */
export function toggledCard(current: string[] | null, id: string): string[] | null {
  const next = current?.includes(id) ? current.filter((c) => c !== id) : [...(current ?? []), id];
  return next.length ? next : null;
}

/** Cartes acceptées par un cinéma, dans l'ordre de `known` ; `active` = dans le filtre. */
export function cardBadges(
  cinemaCards: string[],
  known: Card[],
  filter: string[] | null,
): { card: Card; active: boolean }[] {
  return known
    .filter((card) => cinemaCards.includes(card.id))
    .map((card) => ({ card, active: filter?.includes(card.id) ?? false }));
}

/** « 09/10 » : jour de la liste de la carte, à Paris. */
export function cardListDate(updatedAt: string): string {
  return new Date(updatedAt).toLocaleDateString('fr-FR', {
    day: '2-digit',
    month: '2-digit',
    timeZone: 'Europe/Paris',
  });
}
