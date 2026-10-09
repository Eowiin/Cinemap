// Cartes d'abonnement : « mes cartes » gardées dans le navigateur, et leurs libellés.

import type { Card } from './api/types';
import { isRecord } from './stored';

/** Une carte choisie : son id suffit, le nom vient de `/api/meta`. */
export type MyCard = { id: string };

export function isMyCard(value: unknown): value is MyCard {
  return isRecord(value) && typeof value.id === 'string';
}

/** Cartes d'un cinéma à afficher : toutes (`mine` vide), ou seulement les miennes. */
export function cardBadges(
  cinemaCards: string[],
  known: Card[],
  mine: string[],
  onlyMine: boolean,
): { card: Card; mine: boolean }[] {
  return known
    .filter((card) => cinemaCards.includes(card.id))
    .map((card) => ({ card, mine: mine.includes(card.id) }))
    .filter((badge) => !onlyMine || badge.mine);
}

/** « 09/10 » : jour de la liste de la carte, à Paris. */
export function cardListDate(updatedAt: string): string {
  return new Date(updatedAt).toLocaleDateString('fr-FR', {
    day: '2-digit',
    month: '2-digit',
    timeZone: 'Europe/Paris',
  });
}
