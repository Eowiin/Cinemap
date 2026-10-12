// Noms dans les URLs et titres des pages : mêmes règles que le serveur
// (backend/src/text.rs et api/pages), qui sert ces pages aux moteurs de recherche.
// Un lien généré ici tombe donc directement sur l'URL canonique, sans redirection.

const SLUG_MAX_LEN = 60;
const LIGATURES: Record<string, string> = { œ: 'oe', Œ: 'OE', æ: 'ae', Æ: 'AE', ß: 'ss' };

/** « L'Odyssée » → `l-odyssee` ; vide si rien de latin à garder. */
export function slugify(text: string): string {
  let slug = text
    .replace(/[œŒæÆß]/g, (c) => LIGATURES[c]!)
    .normalize('NFD')
    .replace(/\p{M}/gu, '')
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, '-')
    .replace(/^-+/, '');
  if (slug.length > SLUG_MAX_LEN) {
    const cut = slug.slice(0, SLUG_MAX_LEN + 1).lastIndexOf('-');
    slug = slug.slice(0, cut > 0 ? cut : SLUG_MAX_LEN);
  }
  return slug.replace(/-+$/, '');
}

function containsWord(slug: string, word: string): boolean {
  return `-${slug}-`.includes(`-${word}-`);
}

/** Nom du cinéma, suivi de la ville si le nom ne la contient pas déjà. */
export function cinemaSlug(name: string, city: string | null): string {
  const slug = slugify(name);
  const citySlug = city ? slugify(city) : '';
  if (!citySlug || containsWord(slug, citySlug)) return slug;
  return slug ? `${slug}-${citySlug}` : citySlug;
}

const DISTRICTS: Record<string, { prefix: string; max: number }> = {
  Paris: { prefix: '75', max: 20 },
  Lyon: { prefix: '69', max: 9 },
  Marseille: { prefix: '13', max: 16 },
};

/** Ville affichée, avec l'arrondissement à Paris, Lyon et Marseille (75004 → « Paris 4e »). */
export function locality(city: string, postalCode: string | null): string {
  const district = DISTRICTS[city];
  if (!district || !postalCode || !/^\d{5}$/.test(postalCode)) return city;
  if (!postalCode.startsWith(district.prefix)) return city;
  let n = Number(postalCode.slice(2));
  if (n === 116) n = 16;
  if (n < 1 || n > district.max) return city;
  return n === 1 ? `${city} 1er` : `${city} ${n}e`;
}

export const HOME_TITLE = 'Cinemap : séances de cinéma près de chez vous';

export function movieTitle(title: string): string {
  return `${title} : séances et horaires | Cinemap`;
}

export function cinemaTitle(name: string, city: string | null, postalCode: string | null): string {
  if (city) {
    const place = locality(city, postalCode);
    if (place !== city || !containsWord(slugify(name), slugify(city))) {
      return `${name} (${place}) : programme et horaires | Cinemap`;
    }
  }
  return `${name} : programme et horaires | Cinemap`;
}
