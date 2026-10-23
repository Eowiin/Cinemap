import type { Watch, WatchProvider } from './api/types';

export type WatchRow = { label: string; providers: WatchProvider[] };

const sameNames = (a: WatchProvider[], b: WatchProvider[]) =>
  a.length === b.length && a.every((p, i) => p.name === b[i]?.name);

/** Lignes à afficher, de la moins chère à la plus chère, sans les types vides.
 * Location et achat ont souvent les mêmes plateformes : une seule ligne alors. */
export function watchRows(watch: Watch): WatchRow[] {
  const rows: WatchRow[] = [
    { label: 'Gratuit', providers: watch.free },
    { label: 'Abonnement', providers: watch.flatrate },
  ];
  if (sameNames(watch.rent, watch.buy)) {
    rows.push({ label: 'Location ou achat', providers: watch.rent });
  } else {
    rows.push(
      { label: 'Location', providers: watch.rent },
      { label: 'Achat', providers: watch.buy },
    );
  }
  return rows.filter((row) => row.providers.length > 0);
}
