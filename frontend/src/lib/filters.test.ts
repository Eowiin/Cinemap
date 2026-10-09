import { describe, expect, it } from 'vitest';
import { activeFilters, clearing, weekDays } from './filters';
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
