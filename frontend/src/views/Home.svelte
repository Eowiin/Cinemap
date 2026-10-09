<script lang="ts">
  import FilterBar from '../components/FilterBar.svelte';
  import MyCards from '../components/MyCards.svelte';
  import Icon from '../components/Icon.svelte';
  import Poster from '../components/Poster.svelte';
  import Status from '../components/Status.svelte';
  import { api } from '../lib/api/client';
  import { app, map, PARIS, RADIUS_CHOICES, showtimeFilters } from '../lib/app.svelte';
  import { favorites } from '../lib/favorites.svelte';
  import { formatRuntime, timeAgo, timeOf } from '../lib/format';
  import { router } from '../lib/router.svelte';
  import { resource } from '../lib/resource.svelte';
  import { href } from '../lib/routes';

  const movies = resource(
    () => ({ ...showtimeFilters(), position: app.position, radius_km: app.radiusKm }),
    (args, signal) => api.nowShowing({ ...args, limit: 60 }, signal),
  );

  $effect(() => {
    map.show([]);
  });

  const isParis = $derived(app.position.lat === PARIS.lat && app.position.lng === PARIS.lng);
  const dates = $derived(app.meta?.dates_available ?? []);
</script>

<svelte:head><title>Cinemap · à l'affiche</title></svelte:head>

<section class="intro">
  <h1>À l'affiche</h1>
  <p class="where">
    autour de
    <strong>{app.positionLabel}</strong>,
    <label>
      <span class="visually-hidden">Rayon</span>
      <select value={app.radiusKm} onchange={(e) => app.setRadius(Number(e.currentTarget.value))}>
        {#each RADIUS_CHOICES as km (km)}<option value={km}>{km} km</option>{/each}
      </select>
    </label>
  </p>
  <div class="locate">
    <button class="button" onclick={() => app.locate()} disabled={app.locating}>
      <Icon name="locate" />
      {app.locating ? 'Localisation…' : 'Autour de moi'}
    </button>
    {#if !isParis}
      <button class="button" onclick={() => app.setPosition(PARIS, 'Paris')}>Paris</button>
    {/if}
  </div>
  {#if app.locateError}<p class="error">{app.locateError}</p>{/if}
  <MyCards />
</section>

{#if favorites.cinemas.items.length || favorites.movies.items.length}
  <section class="favorites" aria-labelledby="favorites-title">
    <h2 id="favorites-title" class="visually-hidden">Mes favoris</h2>
    {#if favorites.cinemas.items.length}
      <ul class="scroll-row" aria-label="Mes cinémas">
        {#each favorites.cinemas.items as cinema (cinema.id)}
          <li class="chip fav">
            <Icon name="star" size={14} filled />
            <a href={href({ name: 'cinema', id: cinema.id }, router.filters)}>{cinema.name}</a>
            <button
              aria-label="Retirer {cinema.name} des favoris"
              onclick={() => favorites.cinemas.remove(cinema.id)}
            >
              <Icon name="close" size={14} />
            </button>
          </li>
        {/each}
      </ul>
    {/if}
    {#if favorites.movies.items.length}
      <ul class="scroll-row" aria-label="Mes films">
        {#each favorites.movies.items as movie (movie.id)}
          <li class="chip fav">
            <Icon name="star" size={14} filled />
            <a href={href({ name: 'movie', id: movie.id }, router.filters)}>{movie.title}</a>
            <button
              aria-label="Retirer {movie.title} des favoris"
              onclick={() => favorites.movies.remove(movie.id)}
            >
              <Icon name="close" size={14} />
            </button>
          </li>
        {/each}
      </ul>
    {/if}
  </section>
{/if}

<FilterBar {dates} />

<Status loading={movies.loading && !movies.data} error={movies.error} retry={movies.retry} />

{#if movies.data}
  {#if movies.data.movies.length === 0}
    <div class="empty">
      <p>Aucune séance trouvée avec ces critères.</p>
      {#if app.radiusKm < 100}
        <button class="button" onclick={() => app.setRadius(app.radiusKm < 30 ? 30 : 100)}>
          Chercher plus loin
        </button>
      {/if}
    </div>
  {:else}
    <ol class="movies" class:stale={movies.loading}>
      {#each movies.data.movies as item (item.movie.id)}
        <li>
          <a class="movie" href={href({ name: 'movie', id: item.movie.id }, router.filters)}>
            <Poster url={item.movie.poster_url} title={item.movie.title} width={72} height={96} />
            <div class="info">
              <h2>
                {#if favorites.movies.has(item.movie.id)}<span class="star" title="Favori"
                    ><Icon name="star" size={15} filled /></span
                  >{/if}
                {item.movie.title}
              </h2>
              <p class="muted small">
                {[item.movie.genres.slice(0, 2).join(', '), formatRuntime(item.movie.runtime_min)]
                  .filter(Boolean)
                  .join(' · ')}
              </p>
              <p class="small">
                {item.cinema_count} cinéma{item.cinema_count > 1 ? 's' : ''} · {item.showtime_count}
                séance{item.showtime_count > 1 ? 's' : ''}
              </p>
              <p class="next small">Prochaine à <strong>{timeOf(item.next_showtime)}</strong></p>
            </div>
          </a>
        </li>
      {/each}
    </ol>
  {/if}
{/if}

{#if app.meta?.last_scrape_at}
  <p class="updated muted small">Séances mises à jour {timeAgo(app.meta.last_scrape_at)}</p>
{/if}

<style>
  .intro {
    display: grid;
    gap: 0.4rem;
    margin-bottom: 0.9rem;
  }

  h1 {
    font-size: 1.7rem;
    letter-spacing: -0.02em;
  }

  .where {
    margin: 0;
    color: var(--muted);
  }

  .where strong {
    color: var(--text);
  }

  .where select {
    border: none;
    background: none;
    font-weight: 600;
    color: var(--accent);
    padding: 0;
  }

  .locate {
    display: flex;
    gap: 0.5rem;
  }

  .error {
    margin: 0;
    color: var(--accent);
    font-size: 0.9rem;
  }

  .favorites {
    display: grid;
    gap: 0.4rem;
    margin-bottom: 0.9rem;
  }

  .favorites ul {
    list-style: none;
    margin: 0;
    padding: 0;
  }

  .fav {
    padding-right: 0.3em;
  }

  .fav :global(svg),
  .star :global(svg) {
    color: #e0a526;
    vertical-align: -1px;
  }

  .fav a {
    text-decoration: none;
  }

  .fav button {
    display: inline-flex;
    padding: 0.15em;
    border: none;
    background: none;
    border-radius: 999px;
    color: var(--muted);
  }

  .fav button :global(svg) {
    color: inherit;
  }

  .fav button:hover {
    background: var(--surface-2);
  }

  .movies {
    list-style: none;
    margin: 1rem 0 0;
    padding: 0;
    display: grid;
    gap: 0.5rem;
    transition: opacity 0.15s;
  }

  .stale {
    opacity: 0.55;
  }

  .movie {
    display: flex;
    gap: 0.85rem;
    padding: 0.55rem;
    border-radius: var(--radius);
    text-decoration: none;
  }

  .movie:hover {
    background: var(--surface-2);
  }

  .info {
    min-width: 0;
    display: grid;
    align-content: start;
    gap: 0.15rem;
  }

  h2 {
    font-size: 1.05rem;
  }

  p {
    margin: 0;
  }

  .small {
    font-size: 0.88rem;
  }

  .next strong {
    color: var(--accent);
  }

  .empty {
    padding: 1.5rem 0;
    display: grid;
    gap: 0.6rem;
    justify-items: start;
  }

  .updated {
    margin-top: 1.5rem;
  }
</style>
