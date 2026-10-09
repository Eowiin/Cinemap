import { type FavoriteCinema, isFavoriteCinema } from './favorites';
import { StoredList } from './stored.svelte';

export const favorites = {
  cinemas: new StoredList<FavoriteCinema>('cinemap:favorites:cinemas', isFavoriteCinema),
};

// Films favoris retirés le 2026-10-09 : on efface l'ancienne liste plutôt que de la laisser
// traîner dans le navigateur.
try {
  localStorage.removeItem('cinemap:favorites:movies');
} catch {
  // Stockage indisponible : rien à effacer.
}
