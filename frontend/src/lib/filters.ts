// Barre de filtres : fonctions pures (jours proposés, filtres actifs), testées sans DOM.

import { addDays } from './format';
import type { UrlFilters } from './routes';

/** La semaine à partir d'aujourd'hui, plus le jour choisi s'il est au-delà (lien partagé). */
export function weekDays(today: string, selected: string): string[] {
  const days = Array.from({ length: 7 }, (_, i) => addDays(today, i));
  return selected > days[days.length - 1]! ? [...days, selected] : days;
}

export type ActiveFilter = { key: 'version' | 'after' | 'favorites' | 'cards'; label: string };

/** Filtres actifs (hors date), dans l'ordre du panneau, pour les pastilles retirables. */
export function activeFilters(
  filters: UrlFilters,
  cardNames: Record<string, string>,
  withListFilters: boolean,
): ActiveFilter[] {
  const active: ActiveFilter[] = [];
  if (filters.version) {
    active.push({ key: 'version', label: filters.version === 'VO' ? 'VO / VOST' : 'VF' });
  }
  if (filters.after) {
    active.push({ key: 'after', label: `Après ${filters.after.replace(':00', ' h')}` });
  }
  if (withListFilters && filters.favorites) {
    active.push({ key: 'favorites', label: 'Mes favoris' });
  }
  if (withListFilters && filters.cards) {
    const names = filters.cards.map((id) => cardNames[id] ?? id);
    active.push({ key: 'cards', label: names.join(' ou ') });
  }
  return active;
}

/** Ce qu'il faut changer dans l'URL pour retirer un filtre. */
export function clearing(key: ActiveFilter['key']): Partial<UrlFilters> {
  switch (key) {
    case 'version':
      return { version: null };
    case 'after':
      return { after: null };
    case 'favorites':
      return { favorites: false };
    case 'cards':
      return { cards: null };
  }
}
