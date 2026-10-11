import { describe, expect, it } from 'vitest';
import {
  clock,
  conflicts,
  distanceKm,
  endMinutes,
  isPlanStop,
  link,
  matchesRef,
  linkLabel,
  travelLabel,
  parseShareParam,
  plan,
  shareParam,
  startMinutes,
  travel,
  type PlanStop,
} from './planner';

// Grand Rex et UGC Ciné Cité Les Halles : ~1,2 km à vol d'oiseau.
const REX = { id: 'C0065', name: 'Le Grand Rex', lat: 48.8707, lng: 2.3477 };
const HALLES = { id: 'C0159', name: 'UGC Les Halles', lat: 48.8617, lng: 2.3464 };

function stop(id: string, startsAt: string, runtime: number | null, cinema = REX): PlanStop {
  return {
    id,
    date: '2026-10-11',
    starts_at: startsAt,
    version: 'VF',
    booking_url: null,
    movie: { id: Number(id.replace(/\D/g, '')) || 1, title: `Film ${id}`, runtime_min: runtime },
    cinema,
  };
}

describe('times', () => {
  it('counts minutes from midnight of the cinema day, past midnight included', () => {
    expect(startMinutes(stop('a', '2026-10-11T20:30:00', 100))).toBe(1230);
    expect(startMinutes(stop('a', '2026-10-12T00:15:00', 100))).toBe(1455);
  });

  it('ends after ads and runtime, with a default runtime', () => {
    expect(endMinutes(stop('a', '2026-10-11T20:00:00', 100))).toBe(1200 + 15 + 100);
    expect(endMinutes(stop('a', '2026-10-11T20:00:00', null))).toBe(1200 + 15 + 120);
  });

  it('formats clock times, wrapping past midnight', () => {
    expect(clock(1335)).toBe('22:15');
    expect(clock(1455)).toBe('00:15');
  });
});

describe('travel', () => {
  it('measures distances as the crow flies', () => {
    expect(distanceKm(REX, HALLES)).toBeCloseTo(1.0, 0);
  });

  it('needs no travel within the same cinema', () => {
    expect(travel(REX, REX)).toEqual({ km: 0, minutes: 0, mode: 'same' });
  });

  it('walks short distances and takes transport beyond ~800 m', () => {
    expect(travel(REX, { lat: 48.8707, lng: 2.3545 }).mode).toBe('walk'); // ~500 m
    const far = travel(REX, { lat: 48.8462, lng: 2.3371 }); // Odéon, ~2,8 km
    expect(far.mode).toBe('transit');
    expect(far.minutes).toBe(Math.ceil(10 + far.km * 3));
  });
});

describe('link', () => {
  const first = stop('a', '2026-10-11T18:00:00', 105); // fin 20:00

  it('flags overlaps', () => {
    expect(link(first, stop('b', '2026-10-11T19:45:00', 90)).status).toBe('overlap');
  });

  it('flags a travel that does not fit', () => {
    const l = link(first, stop('b', '2026-10-11T20:05:00', 90, HALLES));
    expect(l.gap).toBe(5);
    expect(l.status).toBe('short');
  });

  it('flags a tight change, then accepts a comfortable one', () => {
    expect(link(first, stop('b', '2026-10-11T20:05:00', 90)).status).toBe('tight');
    expect(link(first, stop('b', '2026-10-11T20:10:00', 90)).status).toBe('ok');
  });
});

describe('plan', () => {
  it('sorts stops and links each one to the next', () => {
    const late = stop('b', '2026-10-12T00:10:00', 90);
    const early = stop('a', '2026-10-11T21:30:00', 120);
    const result = plan([late, early]);
    expect(result.map((r) => r.stop.id)).toEqual(['a', 'b']);
    expect(result[0]!.next?.gap).toBe(25); // fin 23:45, suivante à 00:10
    expect(result[1]!.next).toBeNull();
  });
});

describe('conflicts', () => {
  const stops = [stop('a', '2026-10-11T14:00:00', 90), stop('b', '2026-10-11T20:00:00', 90)];

  it('is empty when the new showtime fits', () => {
    expect(conflicts(stops, stop('c', '2026-10-11T16:30:00', 90))).toEqual([]);
  });

  it('reports the showtimes it overlaps, before and after', () => {
    const found = conflicts(stops, stop('c', '2026-10-11T15:00:00', 300));
    expect(found.map((c) => [c.other.id, c.after, c.link.status])).toEqual([
      ['a', false, 'overlap'],
      ['b', true, 'overlap'],
    ]);
  });

  it('ignores other days and the showtime itself', () => {
    expect(conflicts([{ ...stops[0]!, date: '2026-10-12' }], stops[0]!)).toEqual([]);
    expect(conflicts(stops, stops[0]!)).toEqual([]);
  });
});

describe('share link', () => {
  it('round-trips cinema, movie and start time, in time order', () => {
    const late = stop('7', '2026-10-12T00:15:00', 90, HALLES);
    const early = stop('42', '2026-10-11T14:05:00', 90);
    const param = shareParam([late, early]);
    expect(param).toBe('C0065.42.1405,C0159.7.2415');
    const refs = parseShareParam(param);
    expect(refs).toEqual([
      { cinemaId: 'C0065', movieId: 42, minutes: 845 },
      { cinemaId: 'C0159', movieId: 7, minutes: 1455 },
    ]);
    expect(matchesRef(refs[1]!, late)).toBe(true);
    expect(matchesRef(refs[1]!, early)).toBe(false);
  });

  it('skips malformed parts and duplicates', () => {
    expect(
      parseShareParam('C0065.1.2030,abc,C0065.1.2030,<x>.2.1000,C0159.3.2061,,C0159.3.0900'),
    ).toEqual([
      { cinemaId: 'C0065', movieId: 1, minutes: 1230 },
      { cinemaId: 'C0159', movieId: 3, minutes: 540 },
    ]);
    expect(parseShareParam(null)).toEqual([]);
  });
});

describe('isPlanStop', () => {
  it('accepts a stored stop and rejects damaged ones', () => {
    const s = stop('a', '2026-10-11T20:00:00', 90);
    expect(isPlanStop(JSON.parse(JSON.stringify(s)))).toBe(true);
    expect(isPlanStop({ ...s, version: 'VOSTFR' })).toBe(false);
    expect(isPlanStop({ ...s, cinema: { ...s.cinema, lat: '48' } })).toBe(false);
    expect(isPlanStop(null)).toBe(false);
  });
});

describe('labels', () => {
  const first = stop('a', '2026-10-11T18:00:00', 105); // fin 20:00
  it('describes travel and links', () => {
    expect(travelLabel(travel(REX, REX))).toBe('même cinéma');
    expect(travelLabel({ km: 0.62, minutes: 10, mode: 'walk' })).toBe('10 min à pied (600 m)');
    expect(travelLabel({ km: 2.84, minutes: 19, mode: 'transit' })).toBe(
      '19 min en transports (2,8 km)',
    );
    expect(linkLabel(link(first, stop('b', '2026-10-11T19:45:00', 90)))).toBe(
      'Chevauchement de 15 min',
    );
    expect(linkLabel(link(first, stop('b', '2026-10-11T20:30:00', 90)))).toBe(
      '30 min de battement',
    );
  });
});
