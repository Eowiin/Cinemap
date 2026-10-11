import { describe, expect, it } from 'vitest';
import { compose, composeOrDrop } from './composer';
import type { PlanStop } from './planner';

const REX = { id: 'C0065', name: 'Le Grand Rex', lat: 48.8707, lng: 2.3477 };
const HALLES = { id: 'C0159', name: 'UGC Les Halles', lat: 48.8617, lng: 2.3464 };

/** Séance du film `movie` à `time` (HH:MM), 90 min de film : fin = début + 1 h 45. */
function s(movie: number, time: string, cinema = REX): PlanStop {
  return {
    id: `${movie}-${time}-${cinema.id}`,
    date: '2026-10-17',
    starts_at: `2026-10-17T${time}:00`,
    version: 'VF',
    booking_url: null,
    movie: { id: movie, title: `Film ${movie}`, runtime_min: 90 },
    cinema,
  };
}

const ids = (stops: PlanStop[]) => stops.map((x) => x.id);

describe('compose', () => {
  it('picks the most compact day first', () => {
    const plans = compose(
      [],
      [
        [s(1, '14:00'), s(1, '18:00')],
        [s(2, '16:00'), s(2, '20:30')],
      ],
    );
    // 14:00 (fin 15:45) + 16:00 : 15 min ; 16:00 (fin 17:45) + 18:00 : 15 min, finit plus
    // tard ; 18:00 + 20:30 : 45 min ; 14:00 + 20:30 (285 min) n'entre pas dans les 3.
    expect(plans.map((p) => [ids(p.stops), p.wait])).toEqual([
      [['1-14:00-C0065', '2-16:00-C0065'], 15],
      [['2-16:00-C0065', '1-18:00-C0065'], 15],
      [['1-18:00-C0065', '2-20:30-C0065'], 45],
    ]);
  });

  it('never overlaps nor allows a too short travel', () => {
    const plans = compose([], [[s(1, '14:00')], [s(2, '15:50', HALLES), s(2, '16:30', HALLES)]]);
    // 15:50 aux Halles : 5 min après la fin, 11 min de trajet → écarté.
    expect(plans.map((p) => ids(p.added))).toEqual([['1-14:00-C0065', '2-16:30-C0159']]);
    expect(plans[0]!.travels).toBe(1);
  });

  it('builds around the showtimes already planned', () => {
    const fixed = [s(9, '17:00')]; // 17:00 → 18:45
    const plans = compose(fixed, [[s(1, '14:00'), s(1, '15:30'), s(1, '19:00')]]);
    // Après (19:00, 15 min d'attente) plutôt qu'avant (14:00, 75 min).
    expect(ids(plans[0]!.stops)).toEqual(['9-17:00-C0065', '1-19:00-C0065']);
    expect(ids(plans[0]!.added)).toEqual(['1-19:00-C0065']);
    expect(ids(plans[1]!.stops)).toEqual(['1-14:00-C0065', '9-17:00-C0065']);
    // 15:30 → 17:15 chevauche 17:00 : jamais proposé.
    expect(plans.flatMap((p) => ids(p.added))).not.toContain('1-15:30-C0065');
  });

  it('penalises tight changes', () => {
    const plans = compose([], [[s(1, '14:00')], [s(2, '15:50'), s(2, '16:00')]]);
    // 15:50 : 5 min de battement (juste, pénalisé de 15) ; 16:00 : 15 min, gagne.
    expect(ids(plans[0]!.added)).toEqual(['1-14:00-C0065', '2-16:00-C0065']);
    expect(plans[1]!.tight).toBe(1);
  });

  it('is empty when a movie has no showtime', () => {
    expect(compose([], [[s(1, '14:00')], []])).toEqual([]);
  });
});

describe('composeOrDrop', () => {
  it('drops the movie that does not fit', () => {
    // Le film 2 (14:30) chevauche le 1 et le 3 : impossible de voir les trois.
    const result = composeOrDrop([], [[s(1, '14:00')], [s(2, '14:30')], [s(3, '16:00')]]);
    expect(result.dropped).toBe(1);
    expect(ids(result.plans[0]!.added)).toEqual(['1-14:00-C0065', '3-16:00-C0065']);
  });
});
