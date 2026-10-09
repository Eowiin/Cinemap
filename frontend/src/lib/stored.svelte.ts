import { type Keyed, parseStored, refreshed, serializeStored, toggled } from './stored';

/**
 * Liste réactive gardée dans `localStorage` sous `key`. Sert aux favoris et servira
 * à « mes cartes » : chaque élément garde de quoi s'afficher sans appel à l'API.
 *
 * - Stockage indisponible (navigation privée, quota) : la liste vaut pour la session.
 * - Modifiée dans un autre onglet : relue ici aussi (événement `storage`).
 */
export class StoredList<T extends Keyed> {
  items = $state<T[]>([]);

  private readonly key: string;
  private readonly isItem: (value: unknown) => value is T;

  constructor(key: string, isItem: (value: unknown) => value is T) {
    this.key = key;
    this.isItem = isItem;
    this.items = this.read();
    window.addEventListener('storage', (event) => {
      if (event.key === key) this.items = parseStored(event.newValue, isItem);
    });
  }

  /** Parcours simple : quelques dizaines d'éléments au plus. */
  has(id: T['id']): boolean {
    return this.items.some((item) => item.id === id);
  }

  toggle(item: T) {
    this.write(toggled(this.items, item));
  }

  remove(id: T['id']) {
    this.write(this.items.filter((item) => item.id !== id));
  }

  /** À appeler quand on affiche l'élément : garde son nom à jour s'il est dans la liste. */
  refresh(item: T) {
    const next = refreshed($state.snapshot(this.items) as T[], item);
    if (next) this.write(next);
  }

  private read(): T[] {
    try {
      return parseStored(localStorage.getItem(this.key), this.isItem);
    } catch {
      return [];
    }
  }

  private write(items: T[]) {
    this.items = items;
    try {
      localStorage.setItem(this.key, serializeStored($state.snapshot(items) as T[]));
    } catch {
      // Stockage indisponible : la liste vaut pour la session.
    }
  }
}
