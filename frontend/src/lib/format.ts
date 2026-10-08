// Fonctions pures d'affichage : dates (toujours à Paris), heures, durées, distances.

const PARIS = 'Europe/Paris';

/** Date (`YYYY-MM-DD`) et heure (`HH:MM`) actuelles à Paris, quel que soit le fuseau du navigateur. */
export function parisNow(now: Date = new Date()): { date: string; time: string } {
  const parts = Object.fromEntries(
    new Intl.DateTimeFormat('en-CA', {
      timeZone: PARIS,
      year: 'numeric',
      month: '2-digit',
      day: '2-digit',
      hour: '2-digit',
      minute: '2-digit',
      hourCycle: 'h23',
    })
      .formatToParts(now)
      .map((p) => [p.type, p.value]),
  );
  return {
    date: `${parts.year}-${parts.month}-${parts.day}`,
    time: `${parts.hour}:${parts.minute}`,
  };
}

/** Ajoute `days` jours à une date `YYYY-MM-DD` (calcul en UTC : pas de piège d'heure d'été). */
export function addDays(date: string, days: number): string {
  const d = new Date(`${date}T00:00:00Z`);
  d.setUTCDate(d.getUTCDate() + days);
  return d.toISOString().slice(0, 10);
}

/** « Aujourd'hui », « Demain », sinon « mer. 9 oct. ». */
export function dateLabel(date: string, today: string): string {
  if (date === today) return "Aujourd'hui";
  if (date === addDays(today, 1)) return 'Demain';
  return new Intl.DateTimeFormat('fr-FR', {
    timeZone: 'UTC',
    weekday: 'short',
    day: 'numeric',
    month: 'short',
  }).format(new Date(`${date}T00:00:00Z`));
}

/** `2026-10-08T20:30:00` → `20:30`. `starts_at` est déjà en heure de Paris. */
export function timeOf(startsAt: string): string {
  return startsAt.slice(11, 16);
}

/** Séance après minuit, rattachée au jour ciné précédent (`starts_at` le lendemain). */
export function isAfterMidnight(startsAt: string, date: string): boolean {
  return startsAt.slice(0, 10) > date;
}

/**
 * Heure minimale à envoyer à l'API : l'heure choisie, et pour aujourd'hui jamais avant
 * maintenant (les séances passées ne servent à rien). Recalculée à chaque changement de
 * date (bug de l'ancien site : le filtre restait celui du jour précédent).
 */
export function effectiveAfter(
  date: string,
  today: string,
  nowTime: string,
  chosen: string | null,
): string | null {
  if (date !== today) return chosen;
  if (!chosen) return nowTime;
  return chosen > nowTime ? chosen : nowTime;
}

/**
 * `HH:MM` arrondi au quart d'heure inférieur. Sert de « maintenant » pour l'API : une séance
 * commencée il y a 10 min est encore attrapable (pubs et bandes-annonces), et la requête ne
 * change que 4 fois par heure au lieu de chaque minute (moins d'appels, cache partagé).
 */
export function floorQuarter(time: string): string {
  const minutes = Number(time.slice(3, 5));
  return `${time.slice(0, 3)}${String(minutes - (minutes % 15)).padStart(2, '0')}`;
}

export function formatRuntime(minutes: number | null): string | null {
  if (!minutes) return null;
  const h = Math.floor(minutes / 60);
  const m = minutes % 60;
  if (h === 0) return `${m} min`;
  return `${h} h ${String(m).padStart(2, '0')}`;
}

export function formatDistance(km: number | null): string | null {
  if (km === null) return null;
  if (km < 1) return `${Math.round(km * 1000)} m`;
  return `${km.toLocaleString('fr-FR', { maximumFractionDigits: 1 })} km`;
}

/** « il y a 3 h » à partir d'un instant ISO en UTC. */
export function timeAgo(iso: string, now: Date = new Date()): string {
  const minutes = Math.round((now.getTime() - new Date(iso).getTime()) / 60000);
  if (minutes < 1) return "à l'instant";
  if (minutes < 60) return `il y a ${minutes} min`;
  const hours = Math.round(minutes / 60);
  if (hours < 48) return `il y a ${hours} h`;
  return `il y a ${Math.round(hours / 24)} jours`;
}

/**
 * Affiche AlloCiné redimensionnée par le CDN (`c_{l}_{h}/` après le domaine) :
 * ~11 Ko au lieu de ~270 Ko pour une vignette.
 */
export function posterUrl(url: string | null, width: number, height: number): string | null {
  if (!url) return null;
  return url.replace(/^(https:\/\/[^/]*acsta\.net)\//, `$1/c_${width}_${height}/`);
}

export function year(date: string | null): string | null {
  return date ? date.slice(0, 4) : null;
}
