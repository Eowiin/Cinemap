// Programme du jour : enchaîner des séances. Fonctions pures, testées sans DOM.
// Hypothèses (docs/FRONT.md, « Programme du jour ») : fin = début + pubs + durée du film,
// trajet estimé à vol d'oiseau, marge pour ne pas courir.

import type { MovieSummary, Showtime, Version } from './api/types';
import { isRecord } from './stored';

/** Pubs et bandes-annonces avant le film. */
export const ADS_MIN = 15;
/** Battement souhaité en plus du trajet (billet, pause). En dessous : « juste ». */
export const MARGIN_MIN = 10;
/** Durée supposée quand AlloCiné ne la donne pas (rare : 1 film sur 58 en local). */
export const DEFAULT_RUNTIME_MIN = 120;
/** Au-delà, on ne propose plus de partager (lien trop long, programme irréaliste). */
export const MAX_STOPS = 8;

/** Une séance du programme, avec de quoi l'afficher et calculer les trajets sans l'API. */
export type PlanStop = {
  /** Id de la séance (AlloCiné). */
  id: string;
  /** Jour ciné (une séance à 00h15 appartient à la veille). */
  date: string;
  starts_at: string;
  version: Version;
  booking_url: string | null;
  movie: { id: number; title: string; runtime_min: number | null };
  cinema: { id: string; name: string; lat: number; lng: number };
};

/** Ce qu'on garde d'une séance affichée (fiche film ou cinéma) pour le programme. */
export function toStop(
  date: string,
  showtime: Showtime,
  movie: Pick<MovieSummary, 'id' | 'title' | 'runtime_min'>,
  cinema: PlanStop['cinema'],
): PlanStop {
  return {
    id: showtime.id,
    date,
    starts_at: showtime.starts_at,
    version: showtime.version,
    booking_url: showtime.booking_url,
    movie: { id: movie.id, title: movie.title, runtime_min: movie.runtime_min },
    cinema: { id: cinema.id, name: cinema.name, lat: cinema.lat, lng: cinema.lng },
  };
}

/** Minutes depuis minuit du jour ciné (une séance à 00h15 le lendemain → 1455). */
export function startMinutes(stop: Pick<PlanStop, 'date' | 'starts_at'>): number {
  const days =
    (Date.parse(`${stop.starts_at.slice(0, 10)}T00:00:00Z`) -
      Date.parse(`${stop.date}T00:00:00Z`)) /
    86_400_000;
  return (
    days * 1440 + Number(stop.starts_at.slice(11, 13)) * 60 + Number(stop.starts_at.slice(14, 16))
  );
}

/** Fin estimée, en minutes depuis minuit du jour ciné. */
export function endMinutes(stop: PlanStop): number {
  return startMinutes(stop) + ADS_MIN + (stop.movie.runtime_min ?? DEFAULT_RUNTIME_MIN);
}

/** `1455` → `00:15` (affichage d'une heure de fin, éventuellement après minuit). */
export function clock(minutes: number): string {
  const m = ((minutes % 1440) + 1440) % 1440;
  return `${String(Math.floor(m / 60)).padStart(2, '0')}:${String(m % 60).padStart(2, '0')}`;
}

/** Distance à vol d'oiseau (haversine), en km. */
export function distanceKm(
  a: { lat: number; lng: number },
  b: { lat: number; lng: number },
): number {
  const rad = Math.PI / 180;
  const dLat = (b.lat - a.lat) * rad;
  const dLng = (b.lng - a.lng) * rad;
  const h =
    Math.sin(dLat / 2) ** 2 +
    Math.cos(a.lat * rad) * Math.cos(b.lat * rad) * Math.sin(dLng / 2) ** 2;
  return 2 * 6371 * Math.asin(Math.sqrt(h));
}

export type Travel = { km: number; minutes: number; mode: 'same' | 'walk' | 'transit' };

/**
 * Trajet estimé : à pied à 4 km/h à vol d'oiseau (15 min/km), sinon en transports
 * (10 min d'attente et de correspondances + 3 min/km). Le plus court des deux : à pied
 * sous ~800 m. Grossier mais prudent dans Paris ; à revoir avec l'usage.
 */
export function travel(
  from: { lat: number; lng: number; id?: string },
  to: { lat: number; lng: number; id?: string },
): Travel {
  const km = distanceKm(from, to);
  if ((from.id !== undefined && from.id === to.id) || km < 0.05) {
    return { km: 0, minutes: 0, mode: 'same' };
  }
  const walk = km * 15;
  const transit = 10 + km * 3;
  return walk <= transit
    ? { km, minutes: Math.ceil(walk), mode: 'walk' }
    : { km, minutes: Math.ceil(transit), mode: 'transit' };
}

/**
 * - `overlap` : la séance suivante commence avant la fin de la précédente ;
 * - `short` : pas le temps de faire le trajet ;
 * - `tight` : trajet possible, mais moins de `MARGIN_MIN` de battement ;
 * - `ok`.
 */
export type LinkStatus = 'overlap' | 'short' | 'tight' | 'ok';

export type Link = {
  travel: Travel;
  /** Temps entre la fin estimée et le début de la suivante (négatif : chevauchement). */
  gap: number;
  /** Ce qui reste une fois le trajet fait. */
  slack: number;
  status: LinkStatus;
};

export function link(from: PlanStop, to: PlanStop): Link {
  const t = travel(from.cinema, to.cinema);
  const gap = startMinutes(to) - endMinutes(from);
  const slack = gap - t.minutes;
  const status: LinkStatus =
    gap < 0 ? 'overlap' : slack < 0 ? 'short' : slack < MARGIN_MIN ? 'tight' : 'ok';
  return { travel: t, gap, slack, status };
}

/** Séances d'un jour, dans l'ordre, chacune avec le lien vers la suivante (`null` pour la dernière). */
export function plan(stops: PlanStop[]): { stop: PlanStop; next: Link | null }[] {
  const sorted = [...stops].sort((a, b) => startMinutes(a) - startMinutes(b));
  return sorted.map((stop, i) => ({
    stop,
    next: i + 1 < sorted.length ? link(stop, sorted[i + 1]!) : null,
  }));
}

/**
 * Ce qui coince si on ajoute `candidate` au programme du jour : la séance d'avant et
 * celle d'après, quand le lien n'est pas `ok`. Sert à prévenir avant d'ajouter.
 */
export function conflicts(
  stops: PlanStop[],
  candidate: PlanStop,
): { other: PlanStop; link: Link; after: boolean }[] {
  const start = startMinutes(candidate);
  const others = stops
    .filter((s) => s.id !== candidate.id && s.date === candidate.date)
    .sort((a, b) => startMinutes(a) - startMinutes(b));
  const before = others.filter((s) => startMinutes(s) <= start);
  const after = others.filter((s) => startMinutes(s) > start);
  const result: { other: PlanStop; link: Link; after: boolean }[] = [];
  // Toutes les séances qui chevauchent, pas seulement la voisine (une longue séance peut en couvrir deux).
  for (const other of before) {
    const l = link(other, candidate);
    if (l.status !== 'ok' && (l.status === 'overlap' || other === before[before.length - 1])) {
      result.push({ other, link: l, after: false });
    }
  }
  for (const other of after) {
    const l = link(candidate, other);
    if (l.status !== 'ok' && (l.status === 'overlap' || other === after[0])) {
      result.push({ other, link: l, after: true });
    }
  }
  return result;
}

// Partage par lien, sans serveur : `s=C0159.82697374947,W7504.82697374838`.
// L'id de séance seul ne suffit pas : on la retrouve via le programme de son cinéma.

export type StopRef = { cinemaId: string; showtimeId: string };

export function shareParam(stops: PlanStop[]): string {
  return [...stops]
    .sort((a, b) => startMinutes(a) - startMinutes(b))
    .map((s) => `${s.cinema.id}.${s.id}`)
    .join(',');
}

/** Lecture tolérante : les morceaux mal formés et les doublons sont ignorés. */
export function parseShareParam(value: string | null): StopRef[] {
  if (!value) return [];
  const seen = new Set<string>();
  const refs: StopRef[] = [];
  for (const part of value.split(',')) {
    const match = part.match(/^([A-Za-z0-9]+)\.([A-Za-z0-9_-]+)$/);
    if (!match || seen.has(match[2]!)) continue;
    seen.add(match[2]!);
    refs.push({ cinemaId: match[1]!, showtimeId: match[2]! });
  }
  return refs.slice(0, MAX_STOPS);
}

const isString = (v: unknown): v is string => typeof v === 'string';

export function isPlanStop(value: unknown): value is PlanStop {
  if (!isRecord(value)) return false;
  const { movie, cinema } = value;
  return (
    isString(value.id) &&
    isString(value.date) &&
    /^\d{4}-\d{2}-\d{2}$/.test(value.date) &&
    isString(value.starts_at) &&
    /^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}/.test(value.starts_at) &&
    (value.version === 'VF' || value.version === 'VO' || value.version === 'VOST') &&
    (value.booking_url === null || isString(value.booking_url)) &&
    isRecord(movie) &&
    typeof movie.id === 'number' &&
    isString(movie.title) &&
    (movie.runtime_min === null || typeof movie.runtime_min === 'number') &&
    isRecord(cinema) &&
    isString(cinema.id) &&
    isString(cinema.name) &&
    typeof cinema.lat === 'number' &&
    typeof cinema.lng === 'number'
  );
}

/** « 8 min à pied (600 m) », « 19 min en transports (2,8 km) », « même cinéma ». */
export function travelLabel(t: Travel): string {
  if (t.mode === 'same') return 'même cinéma';
  const distance =
    t.km < 1
      ? `${Math.round(t.km * 10) * 100} m`
      : `${t.km.toLocaleString('fr-FR', { maximumFractionDigits: 1 })} km`;
  return `${t.minutes} min ${t.mode === 'walk' ? 'à pied' : 'en transports'} (${distance})`;
}

/** `45` → `45 min`, `410` → `6 h 50`. */
export function duration(minutes: number): string {
  if (minutes < 60) return `${minutes} min`;
  return `${Math.floor(minutes / 60)} h ${String(minutes % 60).padStart(2, '0')}`;
}

/** Ce qu'on dit de l'enchaînement entre deux séances. */
export function linkLabel(l: Link): string {
  switch (l.status) {
    case 'overlap':
      return `Chevauchement de ${duration(-l.gap)}`;
    case 'short':
      return `Trajet trop court : ${duration(l.gap)} entre les deux, ${travelLabel(l.travel)}`;
    case 'tight':
      return `Juste : ${duration(l.slack)} de battement`;
    case 'ok':
      return `${duration(l.slack)} de battement`;
  }
}
