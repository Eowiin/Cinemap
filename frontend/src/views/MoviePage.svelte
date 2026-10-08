<script lang="ts">
  import FilterBar from '../components/FilterBar.svelte';
  import Icon from '../components/Icon.svelte';
  import Poster from '../components/Poster.svelte';
  import ShowtimeChips from '../components/ShowtimeChips.svelte';
  import Status from '../components/Status.svelte';
  import { api } from '../lib/api/client';
  import { app, map, RADIUS_CHOICES, showtimeFilters } from '../lib/app.svelte';
  import { formatDistance, formatRuntime, year } from '../lib/format';
  import { resource } from '../lib/resource.svelte';
  import { router } from '../lib/router.svelte';
  import { href } from '../lib/routes';

  let { id }: { id: number } = $props();

  const movie = resource(
    () => id,
    (id, signal) => api.movie(id, signal),
  );
  const showtimes = resource(
    () => ({ id, ...showtimeFilters(), position: app.position, radius_km: app.radiusKm }),
    ({ id, ...filters }, signal) => api.movieShowtimes(id, filters, signal),
  );

  $effect(() => {
    map.show(showtimes.data?.cinemas.map((c) => c.cinema) ?? []);
  });

  let synopsisOpen = $state(false);
  const m = $derived(movie.data?.id === id ? movie.data : null);
  const usesTmdb = $derived(!!m && (!!m.backdrop_url || !!m.trailer_url || m.rating !== null));
  const nextRadius = $derived(RADIUS_CHOICES.find((km) => km > app.radiusKm));
</script>

<svelte:head><title>{m ? `${m.title} · Cinemap` : 'Cinemap'}</title></svelte:head>

<a class="back" href={href({ name: 'home' }, router.filters)}><Icon name="back" /> À l'affiche</a>

<Status loading={movie.loading && !m} error={movie.error} retry={movie.retry} />

{#if m}
  <article>
    {#if m.backdrop_url}
      <div class="backdrop" style:background-image="url({m.backdrop_url})"></div>
    {/if}
    <header class="head">
      <Poster url={m.poster_url} title={m.title} width={110} height={147} />
      <div class="titles">
        <h1>{m.title}</h1>
        {#if m.original_title && m.original_title !== m.title}
          <p class="muted original">{m.original_title}</p>
        {/if}
        <p class="facts muted">
          {[m.production_year ?? year(m.release_date), formatRuntime(m.runtime_min), m.countries[0]]
            .filter(Boolean)
            .join(' · ')}
        </p>
        {#if m.genres.length}
          <p class="genres">
            {#each m.genres as genre (genre)}<span class="badge">{genre}</span>{/each}
          </p>
        {/if}
        {#if m.certificate}<p><span class="badge accent">{m.certificate}</span></p>{/if}
        <p class="ratings">
          {#if m.user_rating !== null}
            <span title="Note des spectateurs AlloCiné"
              ><Icon name="star" size={14} />
              {m.user_rating.toLocaleString('fr-FR', { maximumFractionDigits: 1 })}/5
              <span class="muted">spectateurs</span></span
            >
          {/if}
          {#if m.rating !== null}
            <span title="Note TMDB"
              ><Icon name="star" size={14} />
              {m.rating.toLocaleString('fr-FR', { maximumFractionDigits: 1 })}/10
              <span class="muted">TMDB</span></span
            >
          {/if}
        </p>
      </div>
    </header>

    {#if m.trailer_url}
      <a class="button primary trailer" href={m.trailer_url} target="_blank" rel="noopener">
        <Icon name="play" size={16} /> Bande-annonce
      </a>
    {/if}

    {#if m.directors.length}
      <p class="people"><span class="muted">De</span> {m.directors.join(', ')}</p>
    {/if}
    {#if m.cast.length}
      <p class="people">
        <span class="muted">Avec</span>
        {m.cast
          .slice(0, 6)
          .map((p) => p.name)
          .join(', ')}
      </p>
    {/if}

    {#if m.synopsis}
      <p class="synopsis" class:open={synopsisOpen}>{m.synopsis}</p>
      {#if m.synopsis.length > 220}
        <button class="link" onclick={() => (synopsisOpen = !synopsisOpen)}>
          {synopsisOpen ? 'Réduire' : 'Lire la suite'}
        </button>
      {/if}
    {/if}
  </article>

  <section class="sessions">
    <h2>Séances à moins de {app.radiusKm} km de {app.positionLabel}</h2>
    <FilterBar dates={showtimes.data?.dates ?? []} />
    <Status
      loading={showtimes.loading && !showtimes.data}
      error={showtimes.error}
      retry={showtimes.retry}
    />
    {#if showtimes.data}
      {#if showtimes.data.cinemas.length === 0}
        <div class="empty">
          <p>Aucune séance dans ce rayon avec ces critères.</p>
          {#if nextRadius}
            <button class="button" onclick={() => app.setRadius(nextRadius)}>
              Chercher à {nextRadius} km
            </button>
          {/if}
        </div>
      {:else}
        <ul class="cinemas" class:stale={showtimes.loading}>
          {#each showtimes.data.cinemas as { cinema, showtimes: list } (cinema.id)}
            <li class="cinema">
              <div class="cinema-head">
                <a href={href({ name: 'cinema', id: cinema.id }, router.filters)}>{cinema.name}</a>
                <span class="muted small"
                  >{[cinema.city, formatDistance(cinema.distance_km)]
                    .filter(Boolean)
                    .join(' · ')}</span
                >
              </div>
              <ShowtimeChips showtimes={list} date={showtimes.data.date} />
            </li>
          {/each}
        </ul>
      {/if}
    {/if}
  </section>

  {#if usesTmdb}
    <p class="tmdb muted small">
      Image, bande-annonce et note TMDB : this product uses the TMDB API but is not endorsed or
      certified by TMDB.
    </p>
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

  article {
    position: relative;
    display: grid;
    gap: 0.7rem;
  }

  .backdrop {
    margin: -0.25rem -1rem 0;
    height: 170px;
    background-size: cover;
    background-position: center 30%;
    mask-image: linear-gradient(to bottom, #000 55%, transparent);
  }

  .backdrop + .head {
    margin-top: -70px;
  }

  .head {
    display: flex;
    gap: 1rem;
    align-items: flex-end;
  }

  .head :global(.poster) {
    box-shadow: var(--shadow);
  }

  .titles {
    display: grid;
    gap: 0.3rem;
    min-width: 0;
  }

  h1 {
    font-size: 1.5rem;
    letter-spacing: -0.02em;
  }

  p {
    margin: 0;
  }

  .original {
    font-style: italic;
  }

  .genres {
    display: flex;
    flex-wrap: wrap;
    gap: 0.3rem;
  }

  .ratings {
    display: flex;
    gap: 0.9rem;
    font-size: 0.9rem;
  }

  .ratings :global(svg) {
    color: #e0a526;
    vertical-align: -2px;
  }

  .trailer {
    justify-self: start;
  }

  .people {
    font-size: 0.93rem;
  }

  .synopsis {
    display: -webkit-box;
    -webkit-line-clamp: 4;
    line-clamp: 4;
    -webkit-box-orient: vertical;
    overflow: hidden;
  }

  .synopsis.open {
    display: block;
  }

  .link {
    justify-self: start;
    border: none;
    background: none;
    padding: 0;
    color: var(--accent);
    font-weight: 600;
  }

  .sessions {
    margin-top: 1.6rem;
    display: grid;
    gap: 0.7rem;
  }

  h2 {
    font-size: 1.15rem;
  }

  .cinemas {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    gap: 1rem;
    transition: opacity 0.15s;
  }

  .stale {
    opacity: 0.55;
  }

  .cinema {
    display: grid;
    gap: 0.4rem;
  }

  .cinema-head {
    display: flex;
    flex-wrap: wrap;
    align-items: baseline;
    gap: 0.2rem 0.6rem;
  }

  .cinema-head a {
    font-weight: 650;
    text-decoration: none;
  }

  .cinema-head a:hover {
    text-decoration: underline;
  }

  .small {
    font-size: 0.86rem;
  }

  .empty {
    display: grid;
    gap: 0.6rem;
    justify-items: start;
    padding: 0.5rem 0 1rem;
  }

  .tmdb {
    margin-top: 2rem;
  }
</style>
