import { describe, expect, it } from 'vitest';
import { cinemaSlug, cinemaTitle, cityTitle, locality, movieTitle, slugify } from './seo';

// Mêmes cas que backend/src/text.rs et tests/pages.rs : les deux côtés doivent s'accorder.
describe('slugify', () => {
  it('keeps latin letters and digits', () => {
    expect(slugify("L'Odyssée de Pi")).toBe('l-odyssee-de-pi');
    expect(slugify('  Cœur & Âme : 2 ! ')).toBe('coeur-ame-2');
    expect(slugify('...')).toBe('');
    expect(slugify("L'Été dernier")).toBe('l-ete-dernier');
  });

  it('cuts long titles on a word', () => {
    const slug = slugify('cinema '.repeat(20));
    expect(slug.length).toBeLessThanOrEqual(60);
    expect(slug.endsWith('cinema')).toBe(true);
  });
});

describe('cinemaSlug', () => {
  it('adds the city once', () => {
    expect(cinemaSlug('Luminor Hôtel de Ville', 'Paris')).toBe('luminor-hotel-de-ville-paris');
    expect(cinemaSlug('Pathé Lyon', 'Lyon')).toBe('pathe-lyon');
    expect(cinemaSlug('Le Lyonnais', 'Lyon')).toBe('le-lyonnais-lyon');
    expect(cinemaSlug('Le Rex', null)).toBe('le-rex');
  });
});

describe('titles', () => {
  it('names the district', () => {
    expect(locality('Paris', '75004')).toBe('Paris 4e');
    expect(locality('Paris', '75001')).toBe('Paris 1er');
    expect(locality('Paris', '75116')).toBe('Paris 16e');
    expect(locality('Lyon', '69100')).toBe('Lyon');
    expect(locality('Rennes', '35000')).toBe('Rennes');
  });

  it('matches the server titles', () => {
    expect(cityTitle('Paris', 1)).toBe('Cinéma à Paris : séances et horaires | Cinemap');
    expect(cityTitle('Lyon', 17)).toBe(
      'Cinéma à Lyon : séances et horaires des 17 cinémas | Cinemap',
    );
    expect(movieTitle("L'Été dernier")).toBe("L'Été dernier : séances et horaires | Cinemap");
    expect(cinemaTitle('UGC Ciné Cité Les Halles', 'Paris', '75001')).toBe(
      'UGC Ciné Cité Les Halles (Paris 1er) : programme et horaires | Cinemap',
    );
    expect(cinemaTitle('Pathé Lyon', 'Lyon', null)).toBe(
      'Pathé Lyon : programme et horaires | Cinemap',
    );
    expect(cinemaTitle('Pathé Lyon', 'Lyon', '69002')).toBe(
      'Pathé Lyon (Lyon 2e) : programme et horaires | Cinemap',
    );
  });
});
