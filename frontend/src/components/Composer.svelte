<script lang="ts">
  import type { MovieSummary } from '../lib/api/types';
  import { api } from '../lib/api/client';
  import { app, showtimeFilters } from '../lib/app.svelte';
  import { composeOrDrop, type ComposedPlan } from '../lib/composer';
  import { floorQuarter, formatRuntime, timeOf, titleMatches } from '../lib/format';
  import {
    clock,
    duration,
    link,
    linkLabel,
    toStop,
    travelLabel,
    type PlanStop,
  } from '../lib/planner';
  import { programme, wanted } from '../lib/programme.svelte';
  import { resource } from '../lib/resource.svelte';
  import Icon from './Icon.svelte';
  import Poster from './Poster.svelte';
  import Status from './Status.svelte';

  // Programme automatique : on choisit 2 à 4 films, Cinemap propose les meilleures journées
  // (le moins d'attente) autour des séances déjà prévues, qui restent fixes.

  let { date, fixed }: { date: string; fixed: PlanStop[] } = $props();

  const MAX_FILMS = 4;
  const RADII = [2, 5, 10] as const;
  let open = $state(wanted.items.length > 0);
  let radius = $state<number>(5);
  let query = $state('');

  const isToday = $derived(date === app.today);
  const starts = $derived(
    isToday ? ['now', '14:00', '18:00', '20:00'] : ['10:00', '14:00', '18:00'],
  );
  let start = $state<string | null>(null);
  // Par défaut : maintenant aujourd'hui, 14 h un autre jour (pas une journée qui commence à 9 h).
  const startValue = $derived(start && starts.includes(start) ? start : isToday ? 'now' : '14:00');
  const after = $derived(startValue === 'now' ? floorQuarter(app.now.time) : startValue);

  // Films déjà dans la journée : rien à composer pour eux.
  const films = $derived(wanted.items.filter((w) => !fixed.some((s) => s.movie.id === w.id)));

  const searchArgs = $derived.by(() => {
    const filters = showtimeFilters();
    return {
      date,
      version: filters.version,
      cards: filters.cards,
      cinemas: filters.cinemas,
      after,
      position: app.position,
      radius_km: radius,
    };
  });

  // Suggestions : les films à l'affiche autour (souvent déjà en cache depuis l'accueil).
  const showing = resource(
    () => (open ? searchArgs : null),
    (args, signal) =>
      args ? api.nowShowing({ ...args, limit: 60 }, signal) : Promise.resolve(null),
  );
  // Sans film choisi : les plus diffusés autour ; ensuite, seulement en tapant (la liste
  // repousserait les propositions).
  const suggestions = $derived(
    (wanted.items.length && !query.trim() ? [] : (showing.data?.movies ?? []))
      .map((m) => m.movie)
      .filter(
        (m) =>
          !wanted.has(m.id) &&
          !fixed.some((s) => s.movie.id === m.id) &&
          (!query.trim() || titleMatches(m.title, query)),
      )
      .slice(0, query.trim() ? 8 : 6),
  );

  // Séances de chaque film choisi : un appel par film (4 au plus), gardé tant que les
  // réglages ne changent pas (ajouter un film ne redemande pas les autres).
  const fetched: Record<string, PlanStop[]> = {};
  const showtimes = resource(
    () => (open && films.length ? { ...searchArgs, ids: films.map((f) => f.id) } : null),
    async (args, signal) => {
      if (!args) return null;
      const { ids, ...filters } = args;
      return Promise.all(
        ids.map(async (id) => {
          const key = `${id}|${JSON.stringify(filters)}`;
          if (!fetched[key]) {
            const page = await api.movieShowtimes(id, filters, signal);
            fetched[key] = page.cinemas.flatMap(({ cinema, showtimes }) =>
              showtimes.map((s) => toStop(page.date, s, page.movie, cinema)),
            );
          }
          return fetched[key];
        }),
      );
    },
  );

  // Un film sans séance dans ces conditions est signalé, la journée se compose sans lui.
  const lists = $derived(
    showtimes.data && showtimes.data.length === films.length ? showtimes.data : null,
  );
  const missing = $derived(lists ? films.filter((_, i) => lists[i]!.length === 0) : []);
  const available = $derived(lists ? films.filter((_, i) => lists[i]!.length > 0) : []);
  const result = $derived(
    lists && available.length
      ? composeOrDrop(
          fixed,
          lists.filter((l) => l.length > 0),
        )
      : null,
  );

  function addFilm(movie: MovieSummary) {
    if (wanted.items.length >= MAX_FILMS) return;
    wanted.set([
      ...wanted.items,
      { id: movie.id, title: movie.title, poster_url: movie.poster_url },
    ]);
    query = '';
  }

  function choose(plan: ComposedPlan) {
    programme.set([...programme.items, ...($state.snapshot(plan.added) as PlanStop[])]);
    wanted.set([]);
    open = false;
  }

  function close() {
    open = false;
    wanted.set([]);
    query = '';
  }

  const isAdded = (plan: ComposedPlan, stop: PlanStop) => plan.added.some((s) => s.id === stop.id);
</script>

{#if !open}
  <button class="button" onclick={() => (open = true)}>
    <Icon name="sliders" size={16} /> Composer automatiquement
  </button>
{:else}
  <section class="composer card" aria-labelledby="composer-title">
    <header>
      <h2 id="composer-title">Composer automatiquement</h2>
      <button class="icon" aria-label="Fermer" onclick={close}
        ><Icon name="close" size={16} /></button
      >
    </header>

    <div class="films">
      {#each wanted.items as film (film.id)}
        <button
          class="chip on"
          onclick={() => wanted.remove(film.id)}
          aria-label="Retirer {film.title}"
        >
          {film.title}
          <Icon name="close" size={14} />
        </button>
      {/each}
    </div>

    {#if wanted.items.length < MAX_FILMS}
      <input
        class="search"
        type="search"
        placeholder={wanted.items.length
          ? 'Ajouter un autre film'
          : 'Quels films voulez-vous voir ?'}
        aria-label="Ajouter un film"
        bind:value={query}
      />
      {#if suggestions.length}
        <ul class="suggestions">
          {#each suggestions as movie (movie.id)}
            <li>
              <button class="suggestion" onclick={() => addFilm(movie)}>
                <Poster url={movie.poster_url} title={movie.title} width={32} height={43} />
                <span>
                  {movie.title}
                  <span class="muted small">{formatRuntime(movie.runtime_min) ?? ''}</span>
                </span>
                <Icon name="plus" size={16} />
              </button>
            </li>
          {/each}
        </ul>
      {:else if query.trim() && showing.data}
        <p class="muted small">Aucun film à l'affiche à moins de {radius} km ne correspond.</p>
      {/if}
    {/if}

    <div class="settings">
      <div class="scroll-row" role="group" aria-label="À partir de">
        {#each starts as value (value)}
          <button class="chip" aria-pressed={startValue === value} onclick={() => (start = value)}
            >{value === 'now' ? 'Dès maintenant' : `Dès ${value.replace(':00', ' h')}`}</button
          >
        {/each}
      </div>
      <div class="scroll-row" role="group" aria-label="Distance">
        {#each RADII as km (km)}
          <button class="chip" aria-pressed={radius === km} onclick={() => (radius = km)}
            >{km} km</button
          >
        {/each}
        <span class="muted small where">autour de {app.positionLabel}</span>
      </div>
    </div>

    {#if films.length}
      <Status
        loading={showtimes.loading && !showtimes.data}
        error={showtimes.error}
        retry={showtimes.retry}
      />
    {/if}

    {#if missing.length}
      <p class="notice small">
        {missing.map((f) => `« ${f.title} »`).join(', ')}
        {missing.length > 1 ? 'ne passent' : 'ne passe'} pas à moins de {radius} km après {after}.
      </p>
    {/if}
    {#if result}
      {#if result.plans.length === 0}
        <p class="notice small">
          Impossible de caser ces films dans la journée : essayez plus tôt ou plus loin.
        </p>
      {:else}
        {#if result.dropped !== null}
          <p class="notice small">
            Impossible de tout caser ; sans « {available[result.dropped]?.title} » :
          </p>
        {/if}
        <ol class="plans" class:stale={showtimes.loading}>
          {#each result.plans as plan, i (plan.added.map((s) => s.id).join())}
            <li class="plan">
              <ol class="plan-stops">
                {#each plan.stops as stop, j (stop.id)}
                  {@const added = isAdded(plan, stop)}
                  <li class:fixed={!added}>
                    <strong class="time">{timeOf(stop.starts_at)}</strong>
                    <span class="what">
                      <span class="title">{stop.movie.title}</span>
                      <span class="muted small"
                        >{stop.cinema.name} · {stop.version}{added ? '' : ' · déjà prévue'}</span
                      >
                    </span>
                  </li>
                  {#if plan.stops[j + 1]}
                    {@const l = link(stop, plan.stops[j + 1]!)}
                    <li class="between {l.status}">
                      {l.travel.mode === 'same' ? 'Même cinéma' : travelLabel(l.travel)} · {linkLabel(
                        l,
                      ).toLowerCase()}
                    </li>
                  {/if}
                {/each}
              </ol>
              <div class="plan-foot">
                <span class="muted small">
                  Fin {clock(plan.end)} · {plan.travels === 0
                    ? 'sans trajet'
                    : `${plan.travels} trajet${plan.travels > 1 ? 's' : ''}`} · {duration(
                    plan.wait,
                  )} entre les films
                </span>
                <button class="button small" class:primary={i === 0} onclick={() => choose(plan)}
                  >Choisir</button
                >
              </div>
            </li>
          {/each}
        </ol>
      {/if}
    {:else if !films.length && !missing.length}
      <p class="muted small">
        Choisissez 1 à {MAX_FILMS} films : Cinemap cherche les enchaînements avec le moins d'attente{fixed.length
          ? ', autour des séances déjà prévues'
          : ''}.
      </p>
    {/if}
  </section>
{/if}

<style>
  .composer {
    display: grid;
    gap: 0.7rem;
    padding: 0.9rem;
  }

  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }

  h2 {
    font-size: 1.05rem;
  }

  p {
    margin: 0;
  }

  .small {
    font-size: 0.86rem;
  }

  .icon {
    display: inline-flex;
    padding: 0.3rem;
    border: none;
    border-radius: 999px;
    background: none;
    color: var(--muted);
  }

  .icon:hover {
    background: var(--surface-2);
  }

  .films {
    display: flex;
    flex-wrap: wrap;
    gap: 0.4rem;
  }

  .films:empty {
    display: none;
  }

  .on {
    background: var(--accent-soft);
    border-color: var(--accent-soft);
    color: var(--accent);
    font-weight: 600;
  }

  .search {
    width: 100%;
    padding: 0.5rem 0.7rem;
    border: 1px solid var(--border);
    border-radius: 10px;
    background: var(--surface);
  }

  .suggestions {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    gap: 0.3rem;
  }

  .suggestion {
    width: 100%;
    display: flex;
    align-items: center;
    gap: 0.6rem;
    padding: 0.3rem 0.4rem;
    border: none;
    border-radius: 8px;
    background: none;
    text-align: left;
  }

  .suggestion:hover {
    background: var(--surface-2);
  }

  .suggestion > span {
    flex: 1;
    min-width: 0;
    display: grid;
    font-weight: 550;
  }

  .suggestion :global(svg) {
    color: var(--accent);
  }

  .settings {
    display: grid;
    gap: 0.4rem;
  }

  .where {
    align-self: center;
    white-space: nowrap;
  }

  .notice {
    color: var(--accent);
    font-weight: 500;
  }

  .plans {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    gap: 0.7rem;
    transition: opacity 0.15s;
  }

  .stale {
    opacity: 0.55;
  }

  .plan {
    display: grid;
    gap: 0.5rem;
    padding: 0.7rem;
    border: 1px solid var(--border);
    border-radius: 10px;
    background: var(--bg);
  }

  .plan-stops {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    gap: 0.15rem;
  }

  .plan-stops > li:not(.between) {
    display: flex;
    gap: 0.7rem;
    align-items: baseline;
  }

  .time {
    min-width: 3rem;
    font-variant-numeric: tabular-nums;
  }

  .what {
    display: grid;
    min-width: 0;
  }

  .title {
    font-weight: 650;
  }

  .fixed {
    opacity: 0.6;
  }

  .between {
    margin-left: 1.2rem;
    padding: 0.2rem 0 0.2rem 0.8rem;
    border-left: 2px dashed var(--border);
    font-size: 0.82rem;
    color: var(--muted);
  }

  .between.tight {
    color: #a86a00;
    border-left-color: #e0a526;
  }

  .plan-foot {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.6rem;
  }

  .button.small {
    padding: 0.35em 0.9em;
    font-size: 0.9rem;
  }

  @media (prefers-color-scheme: dark) {
    .between.tight {
      color: #e8b04a;
    }
  }
</style>
