import { isAbort, latest } from './api/client';

/**
 * Données chargées depuis l'API, rechargées quand les arguments changent.
 * À appeler pendant l'initialisation d'un composant (utilise `$effect`).
 *
 * - Les arguments sont comparés par valeur (JSON) : un objet recréé à l'identique ne
 *   relance pas la requête.
 * - La requête précédente est annulée : une réponse lente n'écrase jamais une plus récente.
 * - Les anciennes données restent affichées pendant le chargement (pas de clignotement).
 */
export function resource<A, T>(args: () => A, load: (args: A, signal: AbortSignal) => Promise<T>) {
  const state = $state({
    data: null as T | null,
    error: null as unknown,
    loading: true,
    retry: () => {},
  });
  const run = latest();
  const key = $derived(JSON.stringify(args()));
  let attempt = $state(0);
  state.retry = () => attempt++;

  $effect(() => {
    const current = JSON.parse(key) as A;
    void attempt;
    state.loading = true;
    state.error = null;
    run((signal) => load(current, signal)).then(
      (data) => {
        state.data = data;
        state.loading = false;
      },
      (error) => {
        if (isAbort(error)) return;
        state.error = error;
        state.loading = false;
      },
    );
  });

  return state;
}
