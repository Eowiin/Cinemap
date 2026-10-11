import { describe, expect, it } from 'vitest';
import { dayToIcs, fold, parisToUtc } from './ics';
import type { PlanStop } from './planner';

const stop = (over: Partial<PlanStop> = {}): PlanStop => ({
  id: '82697374947',
  date: '2026-10-12',
  starts_at: '2026-10-12T18:15:00',
  version: 'VO',
  booking_url: 'https://www.allocine.fr/r/1',
  movie: { id: 327174, title: 'Digger, le retour; vraiment', runtime_min: 129 },
  cinema: { id: 'C0065', name: 'Le Grand Rex', lat: 48.8707, lng: 2.3477 },
  ...over,
});

describe('parisToUtc', () => {
  it('handles summer and winter time', () => {
    expect(parisToUtc('2026-10-12', 18 * 60 + 15).toISOString()).toBe('2026-10-12T16:15:00.000Z');
    expect(parisToUtc('2026-11-12', 18 * 60 + 15).toISOString()).toBe('2026-11-12T17:15:00.000Z');
  });

  it('handles showtimes past midnight and the night of the change', () => {
    // 00h15 le lendemain du 24/10 ; changement d'heure dans la nuit du 24 au 25 (3 h → 2 h).
    expect(parisToUtc('2026-10-24', 24 * 60 + 15).toISOString()).toBe('2026-10-24T22:15:00.000Z');
    expect(parisToUtc('2026-10-25', 4 * 60).toISOString()).toBe('2026-10-25T03:00:00.000Z');
  });
});

describe('fold', () => {
  it('keeps lines within 75 bytes, accents included', () => {
    const folded = fold(`DESCRIPTION:${'é'.repeat(100)}`);
    const encoder = new TextEncoder();
    for (const line of folded.split('\r\n'))
      expect(encoder.encode(line).length).toBeLessThanOrEqual(75);
    expect(folded.split('\r\n ').join('')).toBe(`DESCRIPTION:${'é'.repeat(100)}`);
  });
});

describe('dayToIcs', () => {
  it('writes one event per showtime, in UTC, escaped', () => {
    const ics = dayToIcs(
      [stop({ id: 'b', starts_at: '2026-10-12T21:15:00' }), stop({ id: 'a' })],
      new Date('2026-10-11T20:00:00Z'),
    );
    expect(ics.startsWith('BEGIN:VCALENDAR\r\nVERSION:2.0\r\n')).toBe(true);
    expect(ics.endsWith('END:VCALENDAR\r\n')).toBe(true);
    const unfolded = ics.replace(/\r\n /g, '');
    expect(unfolded.match(/UID:[^\r]+/g)).toEqual([
      'UID:a-2026-10-12@cinemap.ethansaux.fr',
      'UID:b-2026-10-12@cinemap.ethansaux.fr',
    ]);
    expect(unfolded).toContain('DTSTART:20261012T161500Z');
    expect(unfolded).toContain('DTEND:20261012T183900Z'); // 18:15 + 15 + 129 = 20:39
    expect(unfolded).toContain('SUMMARY:Digger\\, le retour\\; vraiment (VO)');
    expect(unfolded).toContain('DTSTAMP:20261011T200000Z');
    expect(unfolded).toContain('film vers 18:30');
    expect(unfolded).toContain('\\nRéserver : https://www.allocine.fr/r/1');
  });
});
