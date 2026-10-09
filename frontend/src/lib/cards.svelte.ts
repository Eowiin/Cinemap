import { isMyCard, type MyCard } from './cards';
import { StoredList } from './stored.svelte';

/** Mes cartes d'abonnement : même mécanisme que les favoris. */
export const myCards = new StoredList<MyCard>('cinemap:cards', isMyCard);
