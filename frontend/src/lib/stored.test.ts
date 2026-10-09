import { describe, expect, it } from 'vitest';
import { cardBadges, cardListDate, toggledCard } from './cards';
import { isCoveredPosition, isCoveredPostalCode } from './coverage';
import { isFavoriteCinema, isFavoriteMovie } from './favorites';
import { markedFirst, parseStored, refreshed, serializeStored, toggled } from './stored';

const rex = { id: 'C0065', name: 'Le Grand Rex', city: 'Paris' };
const balzac = { id: 'C0002', name: 'Le Balzac', city: 'Paris' };

describe('parseStored', () => {
  it('relit ce que serializeStored a écrit', () => {
    expect(parseStored(serializeStored([rex, balzac]), isFavoriteCinema)).toEqual([rex, balzac]);
  });

  it('rend une liste vide pour un stockage absent, abîmé ou d’une autre version', () => {
    expect(parseStored(null, isFavoriteCinema)).toEqual([]);
    expect(parseStored('{pas du json', isFavoriteCinema)).toEqual([]);
    expect(parseStored('[]', isFavoriteCinema)).toEqual([]);
    expect(parseStored('{"v":2,"items":[]}', isFavoriteCinema)).toEqual([]);
    expect(parseStored('{"v":1,"items":{}}', isFavoriteCinema)).toEqual([]);
  });

  it('écarte les éléments invalides et les doublons, garde le reste', () => {
    const raw = JSON.stringify({
      v: 1,
      items: [rex, { id: 42, name: 'mauvais type' }, null, { ...rex, name: 'doublon' }, balzac],
    });
    expect(parseStored(raw, isFavoriteCinema)).toEqual([rex, balzac]);
  });

  it('vérifie les films', () => {
    expect(isFavoriteMovie({ id: 1000, title: 'Dune', poster_url: null })).toBe(true);
    expect(isFavoriteMovie({ id: '1000', title: 'Dune', poster_url: null })).toBe(false);
    expect(isFavoriteMovie({ id: 1.5, title: 'Dune', poster_url: null })).toBe(false);
  });
});

describe('toggled', () => {
  it('ajoute en tête puis retire', () => {
    const once = toggled([balzac], rex);
    expect(once).toEqual([rex, balzac]);
    expect(toggled(once, rex)).toEqual([balzac]);
  });
});

describe('refreshed', () => {
  it('met à jour un élément présent sans changer l’ordre', () => {
    const renamed = { ...rex, name: 'Grand Rex' };
    expect(refreshed([balzac, rex], renamed)).toEqual([balzac, renamed]);
  });

  it('ne réécrit rien si l’élément est absent ou identique', () => {
    expect(refreshed([balzac], rex)).toBeNull();
    expect(refreshed([rex], { ...rex })).toBeNull();
  });
});

describe('markedFirst', () => {
  it('garde l’ordre d’origine dans chaque groupe', () => {
    expect(markedFirst([1, 2, 3, 4, 5], (n) => n % 2 === 0)).toEqual([2, 4, 1, 3, 5]);
  });
});

describe('cards', () => {
  const known = [
    { id: 'pathe_cinepass', name: 'Pathé CinéPass', updated_at: '2026-10-09T02:00:00Z' },
    { id: 'ugc_illimite', name: 'UGC Illimité', updated_at: '2026-10-09T02:00:00Z' },
  ];

  it('montre les cartes du cinéma, celles du filtre marquées', () => {
    const badges = cardBadges(['ugc_illimite', 'pathe_cinepass'], known, ['ugc_illimite']);
    expect(badges.map((b) => [b.card.id, b.active])).toEqual([
      ['pathe_cinepass', false],
      ['ugc_illimite', true],
    ]);
    expect(cardBadges([], known, null)).toEqual([]);
  });

  it('ajoute et retire une carte du filtre, sans filtre quand il n’en reste aucune', () => {
    expect(toggledCard(null, 'ugc_illimite')).toEqual(['ugc_illimite']);
    expect(toggledCard(['ugc_illimite'], 'pathe_cinepass')).toEqual([
      'ugc_illimite',
      'pathe_cinepass',
    ]);
    expect(toggledCard(['ugc_illimite'], 'ugc_illimite')).toBeNull();
  });

  it('date la liste à Paris', () => {
    // 23 h 30 UTC le 8 = 1 h 30 le 9 à Paris.
    expect(cardListDate('2026-10-08T23:30:00Z')).toBe('09/10');
  });
});

describe('coverage', () => {
  it("reconnaît l'Île-de-France par position et par code postal", () => {
    expect(isCoveredPosition({ lat: 48.8566, lng: 2.3522 })).toBe(true);
    expect(isCoveredPosition({ lat: 45.764, lng: 4.8357 })).toBe(false);
    expect(isCoveredPostalCode('93100')).toBe(true);
    expect(isCoveredPostalCode('69002')).toBe(false);
    expect(isCoveredPostalCode(null)).toBe(true);
  });
});
