<script lang="ts">
  import Icon from '../components/Icon.svelte';
  import Status from '../components/Status.svelte';
  import { api } from '../lib/api/client';
  import { app, map, showtimeFilters } from '../lib/app.svelte';
  import { activeFilters } from '../lib/filters';
  import { formatRuntime, timeOf } from '../lib/format';
  import {
    clock,
    endMinutes,
    followUps,
    linkLabel,
    nearbyStops,
    parseShareParam,
    travelLabel,
  } from '../lib/planner';
  import { programme, showtimeMenu } from '../lib/programme.svelte';
  import { resolveStops } from '../lib/resolve';
  import { resource } from '../lib/resource.svelte';
  import { router } from '../lib/router.svelte';
  import { href } from '../lib/routes';

  // « Et après ? » : ce qu'on peut enchaîner après une séance (`?s=cinéma.film.HHMM`),
  // à proximité, avec les filtres en cours (version, cartes, favoris).

  /** Assez pour couvrir la soirée dans un rayon de quelques km, même à Paris. */
  const LIMIT = 200;
  const SCOPES = [
    { value: 0, label: 'Même cinéma' },
    { value: 1, label: '1 km' },
    { value: 3, label: '3 km' },
    { value: 5, label: '5 km' },
  ] as const;
  let scope = $state<number>(3);
  /** D'abord ce qui commence dans l'heure et demie qui suit la fin ; le reste sur demande. */
  const SOON_MIN = 90;
  let showLater = $state(false);

  const ref = $derived(parseShareParam(new URLSearchParams(router.search).get('s'))[0] ?? null);
  const date = $derived(router.filters.date ?? app.today);

  // Un appel au programme du cinéma (souvent déjà en cache : on vient de sa fiche).
  const source = resource(
    () => ({ ref, date }),
    ({ ref, date }, signal) =>
      ref ? resolveStops([ref], date, signal) : Promise.resolve({ stops: [], missing: 0 }),
  );
  const from = $derived(source.data?.stops[0] ?? null);
  const end = $derived(from ? endMinutes(from) : null);
  // Fin après minuit : `after` ne sait pas aller au-delà du jour ciné, et il n'y a plus rien.
  const tooLate = $derived(end !== null && end >= 24 * 60);

  const nearby = resource(
    () => {
      if (!from || end === null || tooLate) return null;
      const filters = showtimeFilters();
      return {
        date,
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

  const options = $derived(from && nearby.data ? followUps(from, nearbyStops(nearby.data)) : []);
  const soon = $derived(options.filter((o) => o.link.gap <= SOON_MIN));
  const shown = $derived(showLater || soon.length === 0 ? options : soon);
  const truncated = $derived((nearby.data?.showtimes.length ?? 0) >= LIMIT);
  const runtimes = $derived(new Map(nearby.data?.movies.map((m) => [m.id, m.runtime_min]) ?? []));

  const cardNames = $derived(
    Object.fromEntries((app.meta?.cards ?? []).map((c) => [c.id, c.name])),
  );
  const applied = $derived(
    activeFilters({ ...router.filters, after: null }, cardNames, true).map((f) => f.label),
  );

  $effect(() => {
    if (!from) return;
    // Le cinéma de départ, puis chaque cinéma proposé une fois.
    const cinemas = [from.cinema, ...options.map((o) => o.stop.cinema)].filter(
      (c, i, all) => all.findIndex((x) => x.id === c.id) === i,
    );
    map.show(cinemas, scope === 0 ? from.cinema : null);
  });
</script>

<svelte:head><title>Et après ? · Cinemap</title></svelte:head>

<!-- On arrive d'une fiche ou du programme : le retour y ramène. Lien direct : l'accueil. -->
<a
  class="back"
  href={href({ name: 'home' }, router.filters)}
  onclick={(e) => {
    if (history.length > 1) {
      e.preventDefault();
      history.back();
    }
  }}><Icon name="back" /> Retour</a
>

<header>
  <h1>Et après ?</h1>
  {#if from && end !== null}
    <p>
      Après <strong>{from.movie.title}</strong> à {timeOf(from.starts_at)},
      {from.cinema.name}
      <span class="muted">· fin vers {clock(end)}</span>
    </p>
  {/if}
</header>

<Status loading={source.loading && !source.data} error={source.error} retry={source.retry} />
{#if source.data && !from}
  <p class="empty">Séance introuvable : programme modifié par le cinéma, ou jour passé.</p>
{/if}

{#if from}
  <div class="scroll-row" role="group" aria-label="Distance">
    {#each SCOPES as s (s.value)}
      <button class="chip" aria-pressed={scope === s.value} onclick={() => (scope = s.value)}
        >{s.label}</button
      >
    {/each}
  </div>
  {#if applied.length}
    <p class="muted small">Filtres appliqués : {applied.join(', ')}</p>
  {/if}

  {#if tooLate}
    <p class="empty">Fin après minuit : plus rien à enchaîner ce jour-là.</p>
  {:else}
    <Status loading={nearby.loading && !nearby.data} error={nearby.error} retry={nearby.retry} />
    {#if nearby.data}
      {#if options.length === 0}
        <p class="empty">
          Rien à enchaîner {scope === 0 ? 'dans ce cinéma' : `à moins de ${scope} km`} avec ces critères.
        </p>
      {:else}
        <ul class="options" class:stale={nearby.loading}>
          {#each shown as { stop, link } (stop.id)}
            {@const planned = programme.has(stop.id)}
            <li>
              <button class="option card" class:planned onclick={() => showtimeMenu.open(stop)}>
                <span class="time">
                  <strong>{timeOf(stop.starts_at)}</strong>
                  <span class="version" class:vf={stop.version === 'VF'}>{stop.version}</span>
                </span>
                <span class="info">
                  <span class="title"
                    >{stop.movie.title}{#if planned}<Icon name="check" size={14} />{/if}</span
                  >
                  <span class="muted small">
                    {[
                      link.travel.mode === 'same' ? 'Même cinéma' : stop.cinema.name,
                      formatRuntime(runtimes.get(stop.movie.id) ?? null),
                    ]
                      .filter(Boolean)
                      .join(' · ')}
                  </span>
                  <span class="link {link.status}">
                    {link.travel.mode === 'same' ? '' : `${travelLabel(link.travel)} · `}{linkLabel(
                      link,
                    ).toLowerCase()}
                  </span>
                </span>
              </button>
            </li>
          {/each}
        </ul>
        {#if shown.length < options.length}
          <button class="button more" onclick={() => (showLater = true)}>
            Plus tard ({options.length - shown.length} séances)
          </button>
        {/if}
        {#if truncated && shown.length === options.length}
          <p class="muted small">
            Seules les {LIMIT} séances suivantes sont regardées : réduisez la distance pour voir plus
            loin dans la soirée.
          </p>
        {/if}
      {/if}
    {/if}
  {/if}
{/if}

<style>
  .back {
    display: inline-flex;
    align-items: center;
    gap: 0.2rem;
    text-decoration: none;
    color: var(--muted);
    margin-bottom: 0.6rem;
  }

  header {
    display: grid;
    gap: 0.3rem;
    margin-bottom: 0.9rem;
  }

  h1 {
    font-size: 1.5rem;
    letter-spacing: -0.02em;
  }

  p {
    margin: 0;
  }

  .scroll-row {
    margin-bottom: 0.5rem;
  }

  .small {
    font-size: 0.86rem;
  }

  .empty {
    padding: 1rem 0;
  }

  .options {
    list-style: none;
    margin: 0.8rem 0;
    padding: 0;
    display: grid;
    gap: 0.5rem;
    transition: opacity 0.15s;
  }

  .more {
    margin-bottom: 0.8rem;
  }

  .stale {
    opacity: 0.55;
  }

  .option {
    width: 100%;
    display: flex;
    gap: 0.8rem;
    align-items: flex-start;
    padding: 0.6rem 0.8rem;
    text-align: left;
  }

  .option:hover {
    border-color: var(--accent);
  }

  .planned {
    border-color: var(--accent);
    background: var(--accent-soft);
  }

  .time {
    display: grid;
    min-width: 3rem;
    font-variant-numeric: tabular-nums;
  }

  .time strong {
    font-size: 1.05rem;
  }

  .version {
    font-size: 0.72rem;
    font-weight: 600;
    color: var(--accent);
  }

  .version.vf {
    color: var(--muted);
  }

  .info {
    display: grid;
    min-width: 0;
  }

  .title {
    font-weight: 650;
    display: inline-flex;
    align-items: center;
    gap: 0.3rem;
  }

  .title :global(svg) {
    color: var(--accent);
  }

  .link {
    font-size: 0.84rem;
    color: var(--muted);
  }

  .link.tight {
    color: #a86a00;
  }

  @media (prefers-color-scheme: dark) {
    .link.tight {
      color: #e8b04a;
    }
  }
</style>
