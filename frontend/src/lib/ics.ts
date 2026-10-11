// Export d'une journée vers l'agenda (fichier `.ics`, RFC 5545). Fonctions pures, testées.
// Heures converties en UTC : pas de bloc VTIMEZONE à écrire, et aucun agenda ne se
// trompe de fuseau (les séances sont en heure de Paris, le téléphone peut être ailleurs).

import {
  ADS_MIN,
  DEFAULT_RUNTIME_MIN,
  clock,
  endMinutes,
  startMinutes,
  type PlanStop,
} from './planner';

const PARIS = 'Europe/Paris';

/** Décalage de Paris par rapport à UTC (en minutes) à l'instant `utcMs`. */
function parisOffset(utcMs: number): number {
  const parts = Object.fromEntries(
    new Intl.DateTimeFormat('en-US', {
      timeZone: PARIS,
      hourCycle: 'h23',
      year: 'numeric',
      month: 'numeric',
      day: 'numeric',
      hour: 'numeric',
      minute: 'numeric',
    })
      .formatToParts(new Date(utcMs))
      .map((p) => [p.type, Number(p.value)]),
  );
  const asUtc = Date.UTC(parts.year!, parts.month! - 1, parts.day!, parts.hour!, parts.minute!);
  return Math.round((asUtc - utcMs) / 60_000);
}

/** Minutes depuis minuit du jour ciné `date` (heure de Paris) → instant UTC. */
export function parisToUtc(date: string, minutes: number): Date {
  const naive = Date.parse(`${date}T00:00:00Z`) + minutes * 60_000;
  // Deux passes : le décalage peut changer entre l'heure naïve et l'heure réelle (changement d'heure).
  const first = naive - parisOffset(naive) * 60_000;
  return new Date(naive - parisOffset(first) * 60_000);
}

/** `20261012T161500Z` */
function stamp(d: Date): string {
  return d
    .toISOString()
    .replace(/[-:]/g, '')
    .replace(/\.\d{3}/, '');
}

/** Échappement des valeurs texte (RFC 5545, 3.3.11). */
function text(value: string): string {
  return value
    .replace(/\\/g, '\\\\')
    .replace(/;/g, '\\;')
    .replace(/,/g, '\\,')
    .replace(/\r?\n/g, '\\n');
}

/** Lignes de 75 octets au plus, la suite commence par une espace (RFC 5545, 3.1). */
export function fold(line: string): string {
  const encoder = new TextEncoder();
  const out: string[] = [];
  let current = '';
  let bytes = 0;
  for (const char of line) {
    const size = encoder.encode(char).length;
    const limit = out.length === 0 ? 75 : 74; // l'espace de continuation compte
    if (bytes + size > limit) {
      out.push(current);
      current = '';
      bytes = 0;
    }
    current += char;
    bytes += size;
  }
  out.push(current);
  return out.join('\r\n ');
}

function event(stop: PlanStop, now: Date): string[] {
  const start = parisToUtc(stop.date, startMinutes(stop));
  const end = parisToUtc(stop.date, endMinutes(stop));
  const filmAt = clock(startMinutes(stop) + ADS_MIN);
  const description = [
    `Séance ${clock(startMinutes(stop))}, film vers ${filmAt} (après les pubs).`,
    `Fin estimée ${clock(endMinutes(stop))}${
      stop.movie.runtime_min === null
        ? ` (durée inconnue : ${DEFAULT_RUNTIME_MIN / 60} h supposées)`
        : ''
    }.`,
    ...(stop.booking_url ? [`Réserver : ${stop.booking_url}`] : []),
  ].join('\n');
  return [
    'BEGIN:VEVENT',
    `UID:${stop.id}-${stop.date}@cinemap.ethansaux.fr`,
    `DTSTAMP:${stamp(now)}`,
    `DTSTART:${stamp(start)}`,
    `DTEND:${stamp(end)}`,
    `SUMMARY:${text(`${stop.movie.title} (${stop.version})`)}`,
    `LOCATION:${text(stop.cinema.name)}`,
    `GEO:${stop.cinema.lat.toFixed(6)};${stop.cinema.lng.toFixed(6)}`,
    `DESCRIPTION:${text(description)}`,
    ...(stop.booking_url ? [`URL:${stop.booking_url}`] : []),
    'END:VEVENT',
  ];
}

/** Le fichier `.ics` d'une journée : un événement par séance, fin = fin estimée. */
export function dayToIcs(stops: PlanStop[], now: Date = new Date()): string {
  const lines = [
    'BEGIN:VCALENDAR',
    'VERSION:2.0',
    'PRODID:-//Cinemap//Ma journee//FR',
    'CALSCALE:GREGORIAN',
    'METHOD:PUBLISH',
    ...[...stops].sort((a, b) => startMinutes(a) - startMinutes(b)).flatMap((s) => event(s, now)),
    'END:VCALENDAR',
  ];
  return lines.map(fold).join('\r\n') + '\r\n';
}
