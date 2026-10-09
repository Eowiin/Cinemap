import type { Card } from './api/types';
import { isCardRef, type CardRef } from './cards';
import { router } from './router.svelte';
import { StoredList } from './stored.svelte';

/** Dernier filtre de cartes choisi, réappliqué à la visite suivante (même mécanisme que les favoris). */
const remembered = new StoredList<CardRef>('cinemap:cards', isCardRef);

/** Change le filtre de l'URL et le retient pour la prochaine visite. */
export function setCardFilter(cards: string[] | null) {
  router.setFilters({ cards });
  remembered.set((cards ?? []).map((id) => ({ id })));
}

/**
 * À l'ouverture : sans `cards` dans l'URL (un lien partagé décide), reprend le dernier
 * filtre, limité aux cartes que le serveur connaît encore.
 */
export function restoreCardFilter(known: Card[]) {
  if (router.filters.cards) return;
  const ids = remembered.items.map((c) => c.id).filter((id) => known.some((k) => k.id === id));
  if (ids.length) router.setFilters({ cards: ids });
}
