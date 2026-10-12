<script lang="ts">
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
  import { programme, wanted, type WantedMovie } from '../lib/programme.svelte';
  import { resource } from '../lib/resource.svelte';
  import Icon from './Icon.svelte';
  import Poster from './Poster.svelte';
  import Status from './Status.svelte';

  // Programme automatique, en deux temps : on coche 1 à 4 films (ceux de la journée sont
  // déjà cochés), puis Cinemap propose les meilleures journées (le moins d'attente).
  // Tous les horaires sont libres : « Choisir » remplace les séances du jour.

  let { date, day }: { date: string; day: PlanStop[] } = $props();

  const MAX_FILMS = 4;
  /** Films listés avant « Plus de films ». */
  const FIRST_FILMS = 8;
  const RADII = [2, 5, 10] as const;
  // Rouvert tel quel au retour d'une fiche (films cochés gardés dans le navigateur).
  let open = $state(wanted.items.length > 0);
  let step = $state<'pick' | 'results'>('pick');
  let radius = $state<number>(5);
  let query = $state('');
  let more = $state(false);

  const isToday = $derived(date === app.today);
  const starts = $derived(
    isToday ? ['now', '14:00', '18:00', '20:00'] : ['10:00', '14:00', '18:00'],
  );
  let start = $state<string | null>(null);
  // Par défaut : maintenant aujourd'hui, 14 h un autre jour (pas une journée qui commence à 9 h).
  const startValue = $derived(start && starts.includes(start) ? start : isToday ? 'now' : '14:00');
  const after = $derived(startValue === 'now' ? floorQuarter(app.now.time) : startValue);

  const films = $derived(wanted.items);

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

  // Les films à l'affiche autour (souvent déjà en cache depuis l'accueil).
  const showing = resource(
    () => (open ? searchArgs : null),
    (args, signal) =>
      args ? api.nowShowing({ ...args, limit: 60 }, signal) : Promise.resolve(null),
  );
  type Choice = WantedMovie & { runtime_min: number | null };
  // Les films cochés absents de la liste (ex. séance de la journée avant « dès ») restent en tête.
  const choices = $derived.by((): Choice[] => {
    const listed: Choice[] = (showing.data?.movies ?? []).map(({ movie }) => ({
      id: movie.id,
      title: movie.title,
      poster_url: movie.poster_url,
      runtime_min: movie.runtime_min,
    }));
    const extra = films
      .filter((f) => !listed.some((m) => m.id === f.id))
      .map((f) => ({ ...f, runtime_min: null }));
    const all = [...extra, ...listed];
    return query.trim() ? all.filter((m) => titleMatches(m.title, query)) : all;
  });
  // Un film coché plus bas que les premiers (ex. une séance déjà prévue) remonte en tête :
  // sinon « Composer avec 3 films » n'en montre que 2 cochés.
  const shownChoices = $derived.by(() => {
    if (more || query.trim()) return choices;
    const hidden = choices.slice(FIRST_FILMS).filter((m) => wanted.has(m.id));
    return [...hidden, ...choices.slice(0, FIRST_FILMS)];
  });

  // Séances de chaque film choisi : un appel par film (4 au plus), gardé tant que les
  // réglages ne changent pas.
  const fetched: Record<string, PlanStop[]> = {};
  const showtimes = resource(
    () =>
      open && step === 'results' && films.length
        ? { ...searchArgs, ids: films.map((f) => f.id) }
        : null,
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
          [],
          lists.filter((l) => l.length > 0),
        )
      : null,
  );

  function openComposer() {
    // Les films de la journée sont déjà cochés : composer, c'est recomposer toute la journée.
    if (!wanted.items.length) {
      const seen: WantedMovie[] = [];
      for (const s of day) {
        if (!seen.some((w) => w.id === s.movie.id)) {
          seen.push({ id: s.movie.id, title: s.movie.title, poster_url: null });
        }
      }
      wanted.set(seen.slice(0, MAX_FILMS));
    }
    step = 'pick';
    open = true;
  }

  function toggleFilm(film: Choice) {
    if (wanted.has(film.id)) wanted.remove(film.id);
    else if (wanted.items.length < MAX_FILMS) {
      wanted.set([
        ...wanted.items,
        { id: film.id, title: film.title, poster_url: film.poster_url },
      ]);
    }
  }

  function choose(plan: ComposedPlan) {
    programme.set([
      ...programme.items.filter((s) => s.date !== date),
      ...($state.snapshot(plan.stops) as PlanStop[]),
    ]);
    close();
  }

  function close() {
    open = false;
    wanted.set([]);
    query = '';
    more = false;
  }
</script>

{#if !open}
  <button class="button" onclick={openComposer}>
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

    {#if step === 'pick'}
      <p class="muted small">
        Cochez 1 à {MAX_FILMS} films{day.length
          ? ' (ceux déjà prévus ce jour-là le sont ; tous leurs horaires seront revus)'
          : ''}.
      </p>
      <input
        class="search"
        type="search"
        placeholder="Chercher un film"
        aria-label="Chercher un film"
        bind:value={query}
      />
      <Status
        loading={showing.loading && !showing.data}
        error={showing.error}
        retry={showing.retry}
      />
      {#if shownChoices.length}
        <ul class="choices">
          {#each shownChoices as film (film.id)}
            {@const checked = wanted.has(film.id)}
            <li>
              <button
                class="choice"
                class:checked
                aria-pressed={checked}
                disabled={!checked && films.length >= MAX_FILMS}
                onclick={() => toggleFilm(film)}
              >
                <Poster url={film.poster_url} title={film.title} width={32} height={43} />
                <span>
                  {film.title}
                  <span class="muted small">{formatRuntime(film.runtime_min) ?? ''}</span>
                </span>
                <span class="box" aria-hidden="true">
                  {#if checked}<Icon name="check" size={14} />{/if}
                </span>
              </button>
            </li>
          {/each}
        </ul>
        {#if shownChoices.length < choices.length}
          <button class="link" onclick={() => (more = true)}>
            Plus de films ({choices.length - shownChoices.length})
          </button>
        {/if}
      {:else if showing.data}
        <p class="muted small">Aucun film à l'affiche à moins de {radius} km ne correspond.</p>
      {/if}
      <div class="validate">
        <button
          class="button primary"
          disabled={films.length === 0}
          onclick={() => {
            step = 'results';
            query = '';
          }}
        >
          {films.length === 0
            ? 'Cochez au moins un film'
            : `Composer avec ${films.length} film${films.length > 1 ? 's' : ''}`}
        </button>
      </div>
    {:else}
      <div class="summary">
        <span class="small">{films.map((f) => f.title).join(' · ')}</span>
        <button class="link small" onclick={() => (step = 'pick')}>Modifier</button>
      </div>

      <Status
        loading={showtimes.loading && !showtimes.data}
        error={showtimes.error}
        retry={showtimes.retry}
      />

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
            {#each result.plans as plan, i (plan.stops.map((s) => s.id).join())}
              <li class="plan">
                <ol class="plan-stops">
                  {#each plan.stops as stop, j (stop.id)}
                    <li>
                      <strong class="time">{timeOf(stop.starts_at)}</strong>
                      <span class="what">
                        <span class="title">{stop.movie.title}</span>
                        <span class="muted small">{stop.cinema.name} · {stop.version}</span>
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
                      : `${plan.travels} trajet${plan.travels > 1 ? 's' : ''}`}{plan.stops.length >
                    1
                      ? ` · ${duration(plan.wait)} entre les films`
                      : ''}
                  </span>
                  <button class="button small" class:primary={i === 0} onclick={() => choose(plan)}
                    >{day.length ? 'Remplacer ce jour' : 'Choisir'}</button
                  >
                </div>
              </li>
            {/each}
          </ol>
        {/if}
      {/if}
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

  /* Doigt : zone d'appui d'au moins 40 px (l'icône ne change pas). */
  @media (pointer: coarse) {
    .icon {
      padding: 0.6rem;
    }
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

  .choices {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    gap: 0.3rem;
  }

  .choice {
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

  .choice:hover {
    background: var(--surface-2);
  }

  .choice > span:not(.box) {
    flex: 1;
    min-width: 0;
    display: grid;
    font-weight: 550;
  }

  .choice:disabled {
    opacity: 0.45;
  }

  .box {
    flex: none;
    display: grid;
    place-items: center;
    width: 1.3rem;
    height: 1.3rem;
    border: 1.5px solid var(--border);
    border-radius: 6px;
  }

  .checked .box {
    background: var(--accent);
    border-color: var(--accent);
    color: var(--accent-text);
  }

  .link {
    justify-self: start;
    border: none;
    background: none;
    padding: 0;
    color: var(--accent);
    font-weight: 600;
  }

  .validate {
    display: grid;
    padding-top: 0.3rem;
  }

  .validate .button {
    justify-content: center;
  }

  .validate .button:disabled {
    background: var(--surface-2);
    border-color: var(--border);
    color: var(--muted);
  }

  .summary {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 0.6rem;
    font-weight: 600;
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
