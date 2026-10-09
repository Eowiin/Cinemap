import {
  type FavoriteCinema,
  type FavoriteMovie,
  isFavoriteCinema,
  isFavoriteMovie,
} from './favorites';
import { StoredList } from './stored.svelte';

export const favorites = {
  cinemas: new StoredList<FavoriteCinema>('cinemap:favorites:cinemas', isFavoriteCinema),
  movies: new StoredList<FavoriteMovie>('cinemap:favorites:movies', isFavoriteMovie),
};
