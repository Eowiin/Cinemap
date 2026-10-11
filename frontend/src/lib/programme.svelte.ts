import { addDays, parisNow } from './format';
import { isPlanStop, type PlanStop } from './planner';
import { StoredList } from './stored.svelte';

/** Mon programme : les séances choisies, gardées dans le navigateur, tous jours confondus. */
export const programme = new StoredList<PlanStop>('cinemap:programme', isPlanStop);

// Les jours passés ne servent plus. La veille reste un jour : une séance à 00h15
// appartient encore à la veille (jour ciné).
{
  const yesterday = addDays(parisNow().date, -1);
  const kept = programme.items.filter((s) => s.date >= yesterday);
  if (kept.length !== programme.items.length) programme.set(kept);
}

/** Séances du programme pour un jour. */
export function stopsOf(date: string): PlanStop[] {
  return programme.items.filter((s) => s.date === date);
}

/** Jours qui ont au moins une séance, dans l'ordre. */
export function programmeDays(): string[] {
  return programme.items
    .map((s) => s.date)
    .filter((d, i, all) => all.indexOf(d) === i)
    .sort();
}

/** Séance ouverte dans le menu (réserver, ajouter au programme…), une seule à la fois. */
class ShowtimeMenu {
  stop = $state<PlanStop | null>(null);
  formats = $state<string[]>([]);

  open(stop: PlanStop, formats: string[] = []) {
    this.stop = stop;
    this.formats = formats;
  }

  close() {
    this.stop = null;
  }
}

export const showtimeMenu = new ShowtimeMenu();
