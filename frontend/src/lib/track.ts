import { apiUrl } from './api/client';
import type { Route } from './routes';

// Mesure d'audience : un compteur par jour côté serveur (`POST /api/events`), sans cookie
// ni identifiant. Les noms sont la liste fermée de `backend/src/stats.rs`.

export type TrackedEvent =
  | 'view_home'
  | 'view_city'
  | 'view_movie'
  | 'view_cinema'
  | 'view_programme'
  | 'book'
  | 'add'
  | 'compose'
  | 'share'
  | 'calendar'
  | 'keep_shared';

/** Envoie l'événement sans attendre de réponse ; rien si le navigateur demande à ne pas être suivi. */
export function track(event: TrackedEvent, target?: string | number) {
  const nav = navigator as Navigator & { globalPrivacyControl?: boolean };
  if (nav.webdriver || nav.globalPrivacyControl || !nav.sendBeacon) return;
  nav.sendBeacon(apiUrl('/api/events', { e: event, t: target }));
}

/** L'événement « page vue » d'une route, `null` pour une page introuvable. */
export function viewEvent(route: Route): [TrackedEvent, (string | number)?] | null {
  switch (route.name) {
    case 'home':
      return ['view_home'];
    case 'city':
      return ['view_city', route.slug];
    case 'movie':
      return ['view_movie', route.id];
    case 'cinema':
      return ['view_cinema', route.id];
    case 'programme':
      return ['view_programme'];
    case 'not-found':
      return null;
  }
}
