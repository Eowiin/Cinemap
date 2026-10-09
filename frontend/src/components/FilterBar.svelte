<script lang="ts">
  import type { VersionFilter } from '../lib/api/types';
  import { app } from '../lib/app.svelte';
  import { toggledCard } from '../lib/cards';
  import { setCardFilter } from '../lib/cards.svelte';
  import { favorites } from '../lib/favorites.svelte';
  import { activeFilters, clearing, weekDays, type ActiveFilter } from '../lib/filters';
  import { dateLabel } from '../lib/format';
  import { router } from '../lib/router.svelte';
  import Icon from './Icon.svelte';

  /**
   * `dates` : jours qui ont des séances (les autres jours de la semaine sont grisés).
   * `scope` : `cinema` sur la fiche d'un cinéma, où favoris et cartes n'ont pas de sens.
   */
  let { dates, scope = 'list' }: { dates: string[]; scope?: 'list' | 'cinema' } = $props();

  const listFilters = $derived(scope === 'list');
  const selected = $derived(router.filters.date ?? app.today);
  const days = $derived(weekDays(app.today, selected));
  const cards = $derived(app.meta?.cards ?? []);
  const cardNames = $derived(Object.fromEntries(cards.map((c) => [c.id, c.name])));
  const active = $derived(activeFilters(router.filters, cardNames, listFilters));
  const favoriteCount = $derived(favorites.cinemas.items.length);

  const versions: { value: VersionFilter | null; label: string }[] = [
    { value: null, label: 'Toutes' },
    { value: 'VF', label: 'VF' },
    { value: 'VO', label: 'VO / VOST' },
  ];
  const afters = ['18:00', '20:00', '22:00'];

  let dialog: HTMLDialogElement;

  function pickDate(date: string) {
    // `null` = aujourd'hui : un lien partagé sans date reste valable demain.
    router.setFilters({ date: date === app.today ? null : date });
  }

  function clear(filter: ActiveFilter) {
    if (filter.key === 'cards') setCardFilter(null);
    else router.setFilters(clearing(filter.key));
  }

  function clearAll() {
    for (const filter of active) clear(filter);
  }

  /** Clic sur le fond (hors du panneau) : on ferme, comme une feuille native. */
  function closeOnBackdrop(event: MouseEvent) {
    if (event.target === dialog) dialog.close();
  }
</script>

<div class="filters">
  <div class="scroll-row days" role="group" aria-label="Jour">
    {#each days as day (day)}
      {@const empty = day !== selected && day !== app.today && !dates.includes(day)}
      <button
        class="chip"
        aria-pressed={day === selected}
        disabled={empty}
        title={empty ? 'Pas de séance ce jour-là (pas encore publiée ?)' : undefined}
        onclick={() => pickDate(day)}>{dateLabel(day, app.today)}</button
      >
    {/each}
  </div>

  <div class="scroll-row">
    <button class="chip open" onclick={() => dialog.showModal()} aria-haspopup="dialog">
      <Icon name="sliders" size={16} />
      Filtres{#if active.length}<span class="count">{active.length}</span>{/if}
    </button>
    {#each active as filter (filter.key)}
      <button
        class="chip on"
        aria-label="Retirer le filtre {filter.label}"
        onclick={() => clear(filter)}
      >
        {filter.label}
        <Icon name="close" size={14} />
      </button>
    {/each}
  </div>
</div>

<dialog bind:this={dialog} class="sheet" aria-labelledby="filters-title" onclick={closeOnBackdrop}>
  <div class="sheet-body">
    <header>
      <h2 id="filters-title">Filtres</h2>
      <button class="icon" aria-label="Fermer" onclick={() => dialog.close()}>
        <Icon name="close" />
      </button>
    </header>

    <section>
      <h3>Version</h3>
      <div class="options" role="group" aria-label="Version">
        {#each versions as v (v.label)}
          <button
            class="chip"
            aria-pressed={router.filters.version === v.value}
            onclick={() => router.setFilters({ version: v.value })}>{v.label}</button
          >
        {/each}
      </div>
    </section>

    <section>
      <h3>Horaire</h3>
      <div class="options" role="group" aria-label="Horaire">
        <button
          class="chip"
          aria-pressed={router.filters.after === null}
          onclick={() => router.setFilters({ after: null })}
          >{selected === app.today ? 'Dès maintenant' : 'Toute la journée'}</button
        >
        {#each afters as after (after)}
          <button
            class="chip"
            aria-pressed={router.filters.after === after}
            onclick={() => router.setFilters({ after })}>Après {after.replace(':00', ' h')}</button
          >
        {/each}
      </div>
    </section>

    {#if listFilters}
      <section>
        <h3>Cinémas</h3>
        <label class="switch">
          <input
            type="checkbox"
            checked={router.filters.favorites}
            disabled={favoriteCount === 0 && !router.filters.favorites}
            onchange={(e) => router.setFilters({ favorites: e.currentTarget.checked })}
          />
          <span>
            Seulement mes cinémas favoris
            <span class="muted small">
              {favoriteCount === 0
                ? 'Ajoutez-en avec l’étoile, sur la fiche d’un cinéma.'
                : `${favoriteCount} cinéma${favoriteCount > 1 ? 's' : ''}, quelle que soit la distance`}
            </span>
          </span>
        </label>
      </section>

      {#if cards.length}
        <section>
          <h3>Carte d'abonnement</h3>
          <div class="options" role="group" aria-label="Cinémas qui acceptent la carte">
            {#each cards as card (card.id)}
              <button
                class="chip"
                aria-pressed={router.filters.cards?.includes(card.id) ?? false}
                onclick={() => setCardFilter(toggledCard(router.filters.cards, card.id))}
                >{card.name}</button
              >
            {/each}
          </div>
        </section>
      {/if}
    {/if}

    <footer>
      <button class="button" onclick={clearAll} disabled={active.length === 0}>Tout effacer</button>
      <button class="button primary" onclick={() => dialog.close()}>Voir les séances</button>
    </footer>
  </div>
</dialog>

<style>
  .filters {
    display: grid;
    gap: 0.5rem;
  }

  /* Indique qu'on peut faire défiler les jours sur un petit écran. */
  .days {
    mask-image: linear-gradient(to right, #000 85%, transparent);
  }

  .open {
    font-weight: 600;
  }

  .count {
    display: inline-grid;
    place-items: center;
    min-width: 1.3em;
    height: 1.3em;
    padding: 0 0.3em;
    border-radius: 999px;
    background: var(--accent);
    color: var(--accent-text);
    font-size: 0.78rem;
  }

  .on {
    background: var(--accent-soft);
    border-color: var(--accent-soft);
    color: var(--accent);
    font-weight: 600;
  }

  /* Mobile : feuille qui monte du bas. Bureau : panneau centré. */
  .sheet {
    margin: auto auto 0;
    width: 100%;
    max-width: 100%;
    max-height: 85dvh;
    padding: 0;
    border: none;
    border-radius: 18px 18px 0 0;
    background: var(--surface);
    color: var(--text);
    box-shadow: var(--shadow);
  }

  .sheet::backdrop {
    background: rgb(0 0 0 / 0.45);
  }

  .sheet-body {
    display: grid;
    gap: 1.1rem;
    padding: 1rem 1rem calc(1rem + env(safe-area-inset-bottom));
  }

  @media (min-width: 640px) {
    .sheet {
      margin: auto;
      width: 420px;
      border-radius: var(--radius);
    }
  }

  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }

  h2 {
    font-size: 1.15rem;
  }

  h3 {
    font-size: 0.85rem;
    font-weight: 650;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: var(--muted);
    margin-bottom: 0.45rem;
  }

  .options {
    display: flex;
    flex-wrap: wrap;
    gap: 0.4rem;
  }

  .icon {
    display: inline-flex;
    padding: 0.35rem;
    border: none;
    border-radius: 999px;
    background: none;
  }

  .icon:hover {
    background: var(--surface-2);
  }

  .switch {
    display: flex;
    gap: 0.65rem;
    align-items: flex-start;
    cursor: pointer;
  }

  .switch input {
    margin-top: 0.2rem;
    width: 1.1rem;
    height: 1.1rem;
    accent-color: var(--accent);
  }

  .switch > span {
    display: grid;
  }

  .small {
    font-size: 0.85rem;
  }

  footer {
    display: flex;
    justify-content: space-between;
    gap: 0.6rem;
    padding-top: 0.3rem;
  }

  footer .primary {
    flex: 1;
    justify-content: center;
  }

  footer button:disabled {
    opacity: 0.45;
  }
</style>
