<script lang="ts">
  import Composer from '../components/Composer.svelte';
  import FilterBar from '../components/FilterBar.svelte';
  import FollowUps from '../components/FollowUps.svelte';
  import Icon from '../components/Icon.svelte';
  import Status from '../components/Status.svelte';
  import { app, map } from '../lib/app.svelte';
  import { weekDays } from '../lib/filters';
  import { dateLabel, timeOf } from '../lib/format';
  import {
    clock,
    dayRoute,
    directionsUrl,
    DEFAULT_RUNTIME_MIN,
    endMinutes,
    linkLabel,
    MAX_STOPS,
    parseShareParam,
    plan,
    shareParam,
    travelLabel,
    type PlanStop,
  } from '../lib/planner';
  import { dayToIcs } from '../lib/ics';
  import { programme, programmeDays, stopsOf } from '../lib/programme.svelte';
  import { resolveStops } from '../lib/resolve';
  import { resource } from '../lib/resource.svelte';
  import { router } from '../lib/router.svelte';
  import { cinemaRoute, href, movieRoute } from '../lib/routes';

  // Deux modes : mon programme (gardé dans le navigateur), ou un programme partagé
  // (`?date=…&s=C0159.1000032855.2030…`), retrouvé via le programme de chaque cinéma.

  const refs = $derived(parseShareParam(new URLSearchParams(router.search).get('s')));
  const shared = $derived(refs.length > 0);

  // Sans date dans l'URL : aujourd'hui, ou le premier jour prévu s'il n'y a rien aujourd'hui.
  const days = $derived(programmeDays().filter((d) => d >= app.today));
  const date = $derived(
    router.filters.date ??
      (shared || stopsOf(app.today).length ? app.today : (days[0] ?? app.today)),
  );
  // Toute la semaine (on prépare n'importe quel jour), plus les jours prévus au-delà.
  const dayChoices = $derived([...new Set([...weekDays(app.today, date), ...days])].sort());

  const sharedStops = resource(
    () => ({ refs, date }),
    ({ refs, date }, signal) => resolveStops(refs, date, signal),
  );

  const stops = $derived(shared ? (sharedStops.data?.stops ?? []) : stopsOf(date));
  const steps = $derived(plan(stops));
  /** Changements de cinéma dans le jour affiché (0 : pas de trajet à montrer). */
  const moves = $derived(Math.max(dayRoute(stops).path.length - 1, 0));
  const problems = $derived(steps.filter((s) => s.next && s.next.status !== 'ok').length);

  $effect(() => {
    const route = dayRoute(stops);
    map.show(
      route.stops.map((s) => s.cinema),
      null,
      route,
    );
  });

  let copied = $state(false);
  /** Ni partage natif ni presse-papiers : le lien est affiché, à copier à la main. */
  let manualUrl = $state<string | null>(null);

  async function share() {
    const url = `${location.origin}/programme?date=${date}&s=${shareParam(stops)}`;
    const title = `Programme ciné, ${dateLabel(date, app.today).toLowerCase()}`;
    if (navigator.share) {
      try {
        await navigator.share({ title, url });
      } catch {
        // Partage annulé : rien à faire.
      }
      return;
    }
    try {
      await navigator.clipboard.writeText(url);
      copied = true;
      setTimeout(() => (copied = false), 2500);
    } catch {
      manualUrl = url;
    }
  }

  function keepShared() {
    const added = stops.filter((s) => !programme.has(s.id));
    programme.set([...programme.items, ...($state.snapshot(added) as PlanStop[])]);
    router.goto(`/programme${date === app.today ? '' : `?date=${date}`}`);
  }

  /** Fichier `.ics` : sur iPhone, Safari propose « Tout ajouter » au calendrier. */
  function exportCalendar() {
    const blob = new Blob([dayToIcs($state.snapshot(stops) as PlanStop[])], {
      type: 'text/calendar;charset=utf-8',
    });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = `cinemap-${date}.ics`;
    a.click();
    setTimeout(() => URL.revokeObjectURL(url), 10_000);
  }

  function clearDay() {
    programme.set(programme.items.filter((s) => s.date !== date));
  }

  function pickDate(day: string) {
    router.setFilters({ date: day === app.today ? null : day });
  }
</script>

<svelte:head><title>{shared ? 'Programme partagé' : 'Mon programme'} · Cinemap</title></svelte:head>

<a class="back" href={href({ name: 'home' }, router.filters)}><Icon name="back" /> À l'affiche</a>

<header>
  <h1>{shared ? 'Programme partagé' : 'Mon programme'}</h1>
  {#if shared}
    <p class="muted">{dateLabel(date, app.today)}</p>
  {:else}
    <div class="scroll-row" role="group" aria-label="Jour">
      {#each dayChoices as day (day)}
        {@const count = stopsOf(day).length}
        <button class="chip" aria-pressed={day === date} onclick={() => pickDate(day)}>
          {dateLabel(day, app.today)}{#if count}<span class="count">{count}</span>{/if}
        </button>
      {/each}
    </div>
    <FilterBar scope="programme" />
  {/if}
  {#if !shared && stopsOf(date).length}
    <div class="actions">
      <button class="button small primary" onclick={share} disabled={stops.length > MAX_STOPS}>
        <Icon name="share" size={15} />
        {copied ? 'Lien copié' : 'Partager'}
      </button>
      <button class="button small" onclick={exportCalendar}>
        <Icon name="calendar" size={15} /> Agenda
      </button>
      <button class="button small" onclick={clearDay}>Vider ce jour</button>
    </div>
    {#if manualUrl}
      <label class="manual">
        <span class="muted small">Lien à copier :</span>
        <input readonly value={manualUrl} onfocus={(e) => e.currentTarget.select()} />
      </label>
    {/if}
  {/if}
</header>

{#if !shared}
  <div class="compose">
    <Composer {date} day={stopsOf(date)} />
  </div>
{/if}

{#if shared}
  <Status
    loading={sharedStops.loading && !sharedStops.data}
    error={sharedStops.error}
    retry={sharedStops.retry}
  />
  {#if sharedStops.data?.missing}
    <p class="notice">
      {sharedStops.data.missing === 1
        ? 'Une séance du lien est introuvable'
        : `${sharedStops.data.missing} séances du lien sont introuvables`} : programme modifié par le
      cinéma, ou jour passé.
    </p>
  {/if}
{/if}

{#if steps.length}
  {#if moves}
    <button class="button small route-toggle" onclick={() => (map.open = true)}>
      <Icon name="map" size={15} /> Voir le trajet sur la carte
    </button>
  {/if}
  {#if problems}
    <p class="notice">
      <Icon name="alert" size={16} />
      {problems === 1
        ? 'Un enchaînement pose problème'
        : `${problems} enchaînements posent problème`}
      : voir ci-dessous.
    </p>
  {/if}

  <ol class="steps" class:stale={shared && sharedStops.loading}>
    {#each steps as { stop, next }, i (stop.id)}
      <li class="stop card">
        <div class="times">
          <strong>{timeOf(stop.starts_at)}</strong>
          <span class="muted small" title="Fin estimée, pubs comprises"
            >→ {clock(endMinutes(stop))}</span
          >
        </div>
        <div class="info">
          <a class="title" href={href(movieRoute(stop.movie), router.filters)}>{stop.movie.title}</a
          >
          <span class="muted small">
            <a href={href(cinemaRoute(stop.cinema), router.filters)}>{stop.cinema.name}</a>
            · {stop.version}{stop.movie.runtime_min === null
              ? ` · durée inconnue (${DEFAULT_RUNTIME_MIN / 60} h supposées)`
              : ''}
          </span>
        </div>
        <div class="stop-actions">
          {#if stop.booking_url}
            <a class="button small" href={stop.booking_url} target="_blank" rel="noopener"
              >Réserver</a
            >
          {/if}
          {#if !shared}
            <button
              class="icon"
              aria-label="Retirer {stop.movie.title} du programme"
              title="Retirer du programme"
              onclick={() => programme.remove(stop.id)}><Icon name="close" size={16} /></button
            >
          {/if}
        </div>
      </li>
      {#if next}
        <li class="link {next.status}">
          {#if next.status !== 'ok'}<Icon name="alert" size={15} />{/if}
          <span>
            {next.status === 'short' || next.status === 'overlap'
              ? linkLabel(next)
              : `${next.travel.mode === 'same' ? 'Même cinéma' : travelLabel(next.travel)} · ${linkLabel(next).toLowerCase()}`}
            {#if next.travel.mode !== 'same' && next.status !== 'overlap'}
              ·
              <a
                href={directionsUrl(stop.cinema, steps[i + 1]!.stop.cinema, next.travel.mode)}
                target="_blank"
                rel="noopener">Itinéraire</a
              >
            {/if}
          </span>
        </li>
      {/if}
    {/each}
    {#if !shared}
      <li class="after">
        <FollowUps from={steps[steps.length - 1]!.stop} />
      </li>
    {/if}
  </ol>

  {#if shared}
    <div class="actions">
      <button class="button primary" onclick={keepShared}>
        <Icon name="plus" size={16} /> Ajouter à mon programme
      </button>
    </div>
  {/if}
  <p class="muted small">
    Fin estimée = séance + 15 min de pubs + durée du film. Trajets à vol d'oiseau : à pied à 4 km/h,
    sinon en transports (10 min + 3 min/km). Battement souhaité : 10 min.
  </p>
{:else if !shared}
  <div class="empty">
    <p>Rien de prévu {date === app.today ? "aujourd'hui" : 'ce jour-là'}.</p>
    <p class="muted">
      Appuyez sur un horaire, dans la fiche d'un film ou d'un cinéma, puis « Ajouter à mon programme
      ». Cinemap signale les séances qui se chevauchent et les trajets trop courts.
    </p>
    <a class="button" href={href({ name: 'home' }, router.filters)}>Voir les films à l'affiche</a>
  </div>
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
    gap: 0.6rem;
    margin-bottom: 1rem;
  }

  h1 {
    font-size: 1.5rem;
    letter-spacing: -0.02em;
  }

  p {
    margin: 0;
  }

  /* Sur mobile, la carte est cachée : ce bouton l'ouvre. Sur ordinateur, elle est déjà là. */
  .route-toggle {
    margin-bottom: 0.8rem;
  }

  @media (min-width: 900px) {
    .route-toggle {
      display: none;
    }
  }

  .link a {
    color: inherit;
    font-weight: 600;
  }

  .compose {
    margin-bottom: 1rem;
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
    font-size: 0.75rem;
  }

  .notice {
    display: flex;
    align-items: center;
    gap: 0.4rem;
    margin-bottom: 0.8rem;
    color: var(--accent);
    font-weight: 500;
  }

  .steps {
    list-style: none;
    margin: 0 0 1rem;
    padding: 0;
    display: grid;
    transition: opacity 0.15s;
  }

  .stale {
    opacity: 0.55;
  }

  .stop {
    display: flex;
    gap: 0.8rem;
    align-items: center;
    padding: 0.7rem 0.8rem;
  }

  .times {
    display: grid;
    min-width: 3.4rem;
    font-variant-numeric: tabular-nums;
  }

  .times strong {
    font-size: 1.1rem;
  }

  .info {
    flex: 1;
    min-width: 0;
    display: grid;
  }

  .title {
    font-weight: 650;
    text-decoration: none;
  }

  .info a:hover {
    text-decoration: underline;
  }

  .info .small a {
    text-decoration: none;
    color: inherit;
  }

  .stop-actions {
    display: flex;
    align-items: center;
    gap: 0.2rem;
  }

  .button.small {
    padding: 0.3em 0.7em;
    font-size: 0.86rem;
  }

  /* Trait vertical entre deux séances, avec le trajet et le battement. */
  .link {
    display: flex;
    align-items: center;
    gap: 0.4rem;
    margin-left: 1.9rem;
    padding: 0.55rem 0 0.55rem 1rem;
    border-left: 2px dashed var(--border);
    font-size: 0.88rem;
    color: var(--muted);
  }

  .link.tight {
    color: #a86a00;
    border-left-color: #e0a526;
  }

  .link.short,
  .link.overlap {
    color: var(--accent);
    border-left-color: var(--accent);
    font-weight: 500;
  }

  .link :global(svg) {
    flex: none;
  }

  .after {
    margin-left: 1.9rem;
    padding: 0.7rem 0 0.2rem 1rem;
    border-left: 2px dashed var(--border);
  }

  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: 0.5rem;
    margin-bottom: 0.8rem;
  }

  .actions button:disabled {
    opacity: 0.45;
  }

  .manual {
    display: grid;
    gap: 0.2rem;
    margin-bottom: 0.8rem;
  }

  .manual input {
    width: 100%;
    padding: 0.4rem 0.5rem;
    border: 1px solid var(--border);
    border-radius: 8px;
    background: var(--surface);
  }

  .icon {
    display: inline-flex;
    padding: 0.35rem;
    border: none;
    border-radius: 999px;
    background: none;
    color: var(--muted);
  }

  .icon:hover {
    background: var(--surface-2);
  }

  .small {
    font-size: 0.86rem;
  }

  /* Doigt : zone d'appui d'au moins 40 px (l'icône ne change pas). */
  @media (pointer: coarse) {
    .icon {
      padding: 0.6rem;
    }
  }

  .empty {
    display: grid;
    gap: 0.6rem;
    justify-items: start;
    padding: 0.5rem 0 1rem;
  }

  @media (prefers-color-scheme: dark) {
    .link.tight {
      color: #e8b04a;
    }
  }
</style>
