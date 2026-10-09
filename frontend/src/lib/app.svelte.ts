// État partagé entre les pages : métadonnées, heure, position, carte.

import { api, type Position } from './api/client';
import type { CinemaSummary, Meta } from './api/types';
import { effectiveAfter, floorQuarter, parisNow } from './format';
import { router } from './router.svelte';

export const PARIS: Position = { lat: 48.8566, lng: 2.3522 };
export const RADIUS_CHOICES = [5, 15, 30, 50, 100] as const;
const STORAGE_KEY = 'cinemap:position';

type StoredPosition = { position: Position; label: string; radiusKm: number };

function load(): StoredPosition | null {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    return raw ? (JSON.parse(raw) as StoredPosition) : null;
  } catch {
    return null;
  }
}

class AppState {
  meta = $state<Meta | null>(null);
  /** Heure de Paris, rafraîchie chaque minute. */
  now = $state(parisNow());
  /** Le jour ciné vient du serveur s'il répond, sinon de l'horloge locale (à Paris). */
  today = $derived(this.meta?.today ?? this.now.date);

  position = $state<Position>(PARIS);
  positionLabel = $state('Paris');
  radiusKm = $state(15);
  locating = $state(false);
  locateError = $state<string | null>(null);

  constructor() {
    const stored = load();
    if (stored) {
      this.position = stored.position;
      this.positionLabel = stored.label;
      this.radiusKm = stored.radiusKm;
    }
    setInterval(() => (this.now = parisNow()), 60_000);
  }

  async loadMeta() {
    try {
      this.meta = await api.meta();
    } catch {
      // La page fonctionne sans : dates calculées localement.
    }
  }

  private save() {
    try {
      localStorage.setItem(
        STORAGE_KEY,
        JSON.stringify({
          position: this.position,
          label: this.positionLabel,
          radiusKm: this.radiusKm,
        }),
      );
    } catch {
      // Stockage indisponible (navigation privée) : la position vaut pour la session.
    }
  }

  setPosition(position: Position, label: string) {
    this.position = position;
    this.positionLabel = label;
    this.save();
  }

  setRadius(km: number) {
    this.radiusKm = km;
    this.save();
  }

  locate() {
    if (!('geolocation' in navigator)) {
      this.locateError = 'Géolocalisation indisponible sur cet appareil';
      return;
    }
    this.locating = true;
    this.locateError = null;
    navigator.geolocation.getCurrentPosition(
      (p) => {
        this.locating = false;
        this.setPosition({ lat: p.coords.latitude, lng: p.coords.longitude }, 'Ma position');
      },
      (error) => {
        this.locating = false;
        this.locateError =
          error.code === error.PERMISSION_DENIED
            ? 'Localisation refusée : autorisez-la dans le navigateur'
            : 'Position introuvable pour le moment';
      },
      { enableHighAccuracy: false, timeout: 10_000, maximumAge: 5 * 60_000 },
    );
  }
}

export const app = new AppState();

/** Ce que la carte doit montrer, fixé par la page affichée. */
class MapState {
  all = $state<CinemaSummary[]>([]);
  /** Cinémas mis en avant (séances du film, cinéma affiché). */
  highlighted = $state<CinemaSummary[]>([]);
  /** Point à centrer (cinéma affiché) ; sinon la carte cadre `highlighted` et la position. */
  focus = $state<Position | null>(null);
  /** Mobile : carte plein écran ouverte. */
  open = $state(false);

  async loadAll() {
    try {
      this.all = await api.cinemas();
    } catch {
      // La carte reste vide, les pages marchent quand même.
    }
  }

  show(highlighted: CinemaSummary[], focus: Position | null = null) {
    this.highlighted = highlighted;
    this.focus = focus;
  }
}

export const map = new MapState();

/** Filtres à envoyer à l'API, recalculés à chaque changement d'URL, de jour ou de quart d'heure. */
export function showtimeFilters() {
  const date = router.filters.date ?? app.today;
  return {
    date,
    version: router.filters.version,
    after: effectiveAfter(date, app.today, floorQuarter(app.now.time), router.filters.after),
    cards: router.filters.cards,
  };
}
