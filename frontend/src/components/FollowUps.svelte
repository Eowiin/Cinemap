<script lang="ts">
  import { api } from '../lib/api/client';
  import { app, showtimeFilters } from '../lib/app.svelte';
  import { activeFilters } from '../lib/filters';
  import { formatRuntime, timeOf } from '../lib/format';
  import {
    clock,
    endMinutes,
    followUps,
    groupFollowUps,
    linkLabel,
    nearbyStops,
    travelLabel,
    type PlanStop,
  } from '../lib/planner';
  import { programme } from '../lib/programme.svelte';
  import { resource } from '../lib/resource.svelte';
  import { router } from '../lib/router.svelte';
  import Poster from './Poster.svelte';
  import Status from './Status.svelte';

  // « Ensuite ? » : sous la dernière séance de la journée, les films qu'on peut enchaîner
  // à proximité, avec les filtres en cours (version, cartes, favoris). Un appui ajoute.

  let { from }: { from: PlanStop } = $props();

  /** Assez pour couvrir la soirée dans un rayon de quelques km, même à Paris. */
  const LIMIT = 200;
  /** Lignes (film + cinéma) affichées avant « autres films ». */
  const FIRST_ROWS = 5;
  /** Horaires par ligne : au-delà, le film est trop loin dans la soirée pour compter. */
  const TIMES_PER_ROW = 4;
  const SCOPES = [
    { value: 0, label: 'Même cinéma' },
    { value: 1, label: '1 km' },
    { value: 3, label: '3 km' },
    { value: 5, label: '5 km' },
  ] as const;
  let scope = $state<number>(3);
  let expanded = $state(false);

  const end = $derived(endMinutes(from));
  // Fin après minuit : `after` ne va pas au-delà du jour ciné, et il n'y a plus rien.
  const tooLate = $derived(end >= 24 * 60);

  const nearby = resource(
    () => {
      if (tooLate) return null;
      const filters = showtimeFilters();
      return {
        date: from.date,
        version: filters.version,
        cards: filters.cards,
        // « Même cinéma » prime sur le filtre favoris (qui ignore la distance).
        cinemas: scope === 0 ? [from.cinema.id] : filters.cinemas,
        after: clock(end),
        position: { lat: from.cinema.lat, lng: from.cinema.lng },
        radius_km: scope || 1,
        limit: LIMIT,
      };
    },
    (args, signal) => (args ? api.showtimes(args, signal) : Promise.resolve(null)),
  );

  const groups = $derived(
    nearby.data ? groupFollowUps(followUps(from, nearbyStops(nearby.data))) : [],
  );
  const shown = $derived(expanded ? groups : groups.slice(0, FIRST_ROWS));
  const movies = $derived(nearby.data?.movies ?? []);
  const movieOf = (id: number) => movies.find((m) => m.id === id) ?? null;

  const cardNames = $derived(
    Object.fromEntries((app.meta?.cards ?? []).map((c) => [c.id, c.name])),
  );
  const applied = $derived(
    activeFilters({ ...router.filters, after: null }, cardNames, true).map((f) => f.label),
  );

  function add(stop: PlanStop) {
    programme.toggle($state.snapshot(stop) as PlanStop);
    expanded = false;
  }
</script>

<section class="follow-ups" aria-labelledby="follow-ups-title">
  <header>
    <h2 id="follow-ups-title">Ensuite ?</h2>
    <span class="muted small">après {clock(end)}</span>
  </header>

  {#if tooLate}
    <p class="muted small">Fin après minuit : plus rien à enchaîner ce jour-là.</p>
  {:else}
    <div class="scroll-row" role="group" aria-label="Distance">
      {#each SCOPES as s (s.value)}
        <button
          class="chip"
          aria-pressed={scope === s.value}
          onclick={() => {
            scope = s.value;
            expanded = false;
          }}>{s.label}</button
        >
      {/each}
    </div>
    {#if applied.length}
      <p class="muted small">Filtres : {applied.join(', ')}</p>
    {/if}

    <Status loading={nearby.loading && !nearby.data} error={nearby.error} retry={nearby.retry} />
    {#if nearby.data && groups.length === 0}
      <p class="muted small">
        Rien à enchaîner {scope === 0 ? 'dans ce cinéma' : `à moins de ${scope} km`} avec ces critères.
      </p>
    {:else if groups.length}
      <ul class="rows" class:stale={nearby.loading}>
        {#each shown as group (group.key)}
          {@const movie = movieOf(group.movieId)}
          {@const first = group.items[0]!}
          <li class="row">
            <Poster
              url={movie?.poster_url ?? null}
              title={first.stop.movie.title}
              width={44}
              height={59}
            />
            <div class="info">
              <span class="title">{first.stop.movie.title}</span>
              <span class="muted small">
                {[
                  first.link.travel.mode === 'same'
                    ? 'Même cinéma'
                    : `${first.stop.cinema.name} · ${travelLabel(first.link.travel)}`,
                  formatRuntime(first.stop.movie.runtime_min),
                ]
                  .filter(Boolean)
                  .join(' · ')}
              </span>
              <span class="times">
                {#each group.items.slice(0, TIMES_PER_ROW) as { stop, link } (stop.id)}
                  <button
                    class="time"
                    class:tight={link.status === 'tight'}
                    title="Ajouter à ma journée : {linkLabel(link).toLowerCase()}"
                    onclick={() => add(stop)}
                  >
                    <strong>{timeOf(stop.starts_at)}</strong>
                    <span class="version" class:vf={stop.version === 'VF'}>{stop.version}</span>
                  </button>
                {/each}
              </span>
            </div>
          </li>
        {/each}
      </ul>
      {#if shown.length < groups.length}
        <button class="link" onclick={() => (expanded = true)}>
          + {groups.length - shown.length} autre{groups.length - shown.length > 1 ? 's' : ''}
        </button>
      {/if}
      <p class="muted small hint">
        Un appui sur un horaire l'ajoute à la journée. En orange : moins de 10 min de battement.
      </p>
    {/if}
  {/if}
</section>

<style>
  .follow-ups {
    display: grid;
    gap: 0.55rem;
  }

  header {
    display: flex;
    align-items: baseline;
    gap: 0.5rem;
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

  .rows {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    gap: 0.75rem;
    transition: opacity 0.15s;
  }

  .stale {
    opacity: 0.55;
  }

  .row {
    display: flex;
    gap: 0.7rem;
  }

  .info {
    min-width: 0;
    display: grid;
    gap: 0.15rem;
    align-content: start;
  }

  .title {
    font-weight: 650;
  }

  .times {
    display: flex;
    flex-wrap: wrap;
    gap: 0.35rem;
    margin-top: 0.2rem;
  }

  .time {
    display: inline-flex;
    align-items: baseline;
    gap: 0.3rem;
    padding: 0.3rem 0.55rem;
    border-radius: 8px;
    border: 1px solid var(--border);
    background: var(--surface);
    font-variant-numeric: tabular-nums;
  }

  .time:hover {
    border-color: var(--accent);
  }

  .time.tight {
    border-color: #e0a526;
  }

  .version {
    font-size: 0.72rem;
    font-weight: 600;
    color: var(--accent);
  }

  .version.vf {
    color: var(--muted);
  }

  .link {
    justify-self: start;
    border: none;
    background: none;
    padding: 0;
    color: var(--accent);
    font-weight: 600;
  }

  .hint {
    margin-top: 0.2rem;
  }
</style>
