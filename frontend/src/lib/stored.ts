// Listes gardées dans le navigateur (`localStorage`), sans compte : favoris, puis
// « mes cartes » (docs/CARTES.md, lot E). Fonctions pures, la version réactive
// (stored.svelte.ts) s'appuie dessus.

export type Keyed = { id: string | number };

/** Format enregistré, versionné pour pouvoir le faire évoluer sans perdre les listes existantes. */
const VERSION = 1;

/**
 * Lit une liste en écartant ce qui est invalide (données abîmées, autre version,
 * doublons) : le stockage du navigateur ne doit jamais casser la page.
 */
export function parseStored<T extends Keyed>(
  raw: string | null,
  isItem: (value: unknown) => value is T,
): T[] {
  if (!raw) return [];
  let data: unknown;
  try {
    data = JSON.parse(raw);
  } catch {
    return [];
  }
  if (!isRecord(data) || data.v !== VERSION || !Array.isArray(data.items)) return [];
  const seen = new Set<T['id']>();
  return data.items.filter((item: unknown): item is T => {
    if (!isItem(item) || seen.has(item.id)) return false;
    seen.add(item.id);
    return true;
  });
}

export function serializeStored<T extends Keyed>(items: T[]): string {
  return JSON.stringify({ v: VERSION, items });
}

/** Retire l'élément s'il est déjà là, sinon l'ajoute en tête (le plus récent d'abord). */
export function toggled<T extends Keyed>(items: T[], item: T): T[] {
  return items.some((x) => x.id === item.id)
    ? items.filter((x) => x.id !== item.id)
    : [item, ...items];
}

/**
 * Met à jour un élément déjà présent (nom ou affiche qui ont changé) sans changer
 * l'ordre. `null` si l'élément est absent ou identique : rien à écrire.
 */
export function refreshed<T extends Keyed>(items: T[], item: T): T[] | null {
  const current = items.find((x) => x.id === item.id);
  if (!current || JSON.stringify(current) === JSON.stringify(item)) return null;
  return items.map((x) => (x.id === item.id ? item : x));
}

/** Les éléments marqués d'abord, l'ordre d'origine conservé dans chaque groupe (ex. distance). */
export function markedFirst<T>(items: T[], isMarked: (item: T) => boolean): T[] {
  return [...items.filter(isMarked), ...items.filter((item) => !isMarked(item))];
}

export function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}
