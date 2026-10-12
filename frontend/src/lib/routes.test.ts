import { describe, expect, it } from 'vitest';
import {
  cinemaRoute,
  filtersQuery,
  href,
  movieRoute,
  parseFilters,
  parseRoute,
  routePath,
} from './routes';

describe('parseRoute', () => {
  it('recognises the pages', () => {
    expect(parseRoute('/')).toEqual({ name: 'home' });
    expect(parseRoute('/film/1000032855')).toEqual({ name: 'movie', id: 1000032855 });
    expect(parseRoute('/cinema/C0159')).toEqual({ name: 'cinema', id: 'C0159' });
    expect(parseRoute('/cinema/C0159/')).toEqual({ name: 'cinema', id: 'C0159' });
    expect(parseRoute('/programme')).toEqual({ name: 'programme' });
  });

  it('reads the id before the readable name', () => {
    expect(parseRoute('/film/1001-l-ete-dernier')).toEqual({
      name: 'movie',
      id: 1001,
      slug: 'l-ete-dernier',
    });
    expect(parseRoute('/cinema/C0013-luminor-hotel-de-ville-paris/')).toEqual({
      name: 'cinema',
      id: 'C0013',
      slug: 'luminor-hotel-de-ville-paris',
    });
  });

  it('builds the same paths as the server', () => {
    expect(routePath(movieRoute({ id: 1001, title: "L'Été dernier" }))).toBe(
      '/film/1001-l-ete-dernier',
    );
    expect(routePath(cinemaRoute({ id: 'LYON', name: 'Pathé Bellecour', city: 'Lyon' }))).toBe(
      '/cinema/LYON-pathe-bellecour-lyon',
    );
    expect(routePath(movieRoute({ id: 7, title: '…' }))).toBe('/film/7');
  });

  it('rejects anything else', () => {
    expect(parseRoute('/film/abc')).toEqual({ name: 'not-found' });
    expect(parseRoute('/nimporte')).toEqual({ name: 'not-found' });
  });
});

describe('filters', () => {
  it('keeps valid values and drops broken ones', () => {
    expect(parseFilters('?date=2026-10-08&version=VO&after=20:00')).toEqual({
      date: '2026-10-08',
      version: 'VO',
      after: '20:00',
      cards: null,
      favorites: false,
    });
    expect(parseFilters('?date=demain&version=vost&after=25:00')).toEqual({
      date: null,
      version: null,
      after: null,
      cards: null,
      favorites: false,
    });
  });

  it('reads card ids, ignoring malformed and duplicate ones', () => {
    expect(parseFilters('?cards=ugc_illimite,pathe_cinepass,ugc_illimite').cards).toEqual([
      'ugc_illimite',
      'pathe_cinepass',
    ]);
    expect(parseFilters('?cards=UGC%20!,').cards).toBeNull();
  });

  it('round-trips through the query string', () => {
    const filters = {
      date: '2026-10-09',
      version: 'VF' as const,
      after: null,
      cards: ['ugc_illimite', 'pathe_cinepass'],
      favorites: true,
    };
    expect(parseFilters(filtersQuery(filters))).toEqual(filters);
    expect(
      filtersQuery({ date: null, version: null, after: null, cards: null, favorites: false }),
    ).toBe('');
  });

  it('builds links that keep the filters', () => {
    expect(
      href(
        { name: 'movie', id: 42 },
        { date: '2026-10-09', version: null, after: null, cards: null, favorites: false },
      ),
    ).toBe('/film/42?date=2026-10-09');
  });
});
