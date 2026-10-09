import { describe, expect, it } from 'vitest';
import { activeFilters, cinemaMatches, clearing, weekDays } from './filters';
import { parseFilters } from './routes';

describe('weekDays', () => {
  it('propose la semaine, et le jour choisi s’il est plus loin', () => {
    const week = weekDays('2026-10-09', '2026-10-09');
    expect(week).toHaveLength(7);
    expect(week[6]).toBe('2026-10-15');
    expect(weekDays('2026-10-09', '2026-10-20')).toHaveLength(8);
  });
});

describe('activeFilters', () => {
  const filters = parseFilters('?version=VO&after=20:00&favoris=1&cards=ugc_illimite,x');
  const names = { ugc_illimite: 'UGC Illimité' };

  it('liste les filtres actifs avec un libellé lisible', () => {
    expect(activeFilters(filters, names, true).map((f) => f.label)).toEqual([
      'VO / VOST',
      'Après 20 h',
      'Mes favoris',
      'UGC Illimité ou x',
    ]);
  });

  it('ignore favoris et cartes sur la fiche d’un cinéma', () => {
    expect(activeFilters(filters, names, false).map((f) => f.key)).toEqual(['version', 'after']);
  });

  it('sait retirer chaque filtre', () => {
    expect(clearing('favorites')).toEqual({ favorites: false });
    expect(clearing('cards')).toEqual({ cards: null });
  });
});

describe('cinemaMatches', () => {
  const halles = { id: 'C0159', cards: ['ugc_illimite'] };
  const rex = { id: 'C0065', cards: [] };

  it('sans filtre de cinéma, tout passe', () => {
    expect(cinemaMatches(rex, parseFilters('?version=VO'), [])).toBe(true);
  });

  it('applique favoris et cartes, ensemble', () => {
    expect(cinemaMatches(rex, parseFilters('?favoris=1'), ['C0065'])).toBe(true);
    expect(cinemaMatches(halles, parseFilters('?favoris=1'), ['C0065'])).toBe(false);
    expect(cinemaMatches(halles, parseFilters('?cards=ugc_illimite'), [])).toBe(true);
    expect(cinemaMatches(rex, parseFilters('?cards=ugc_illimite&favoris=1'), ['C0065'])).toBe(
      false,
    );
  });

  it('ignore le filtre favoris quand il n’y en a aucun', () => {
    expect(cinemaMatches(rex, parseFilters('?favoris=1'), [])).toBe(true);
  });
});
