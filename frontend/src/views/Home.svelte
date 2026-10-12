<script lang="ts">
  import FilterBar from '../components/FilterBar.svelte';
  import CoverageNotice from '../components/CoverageNotice.svelte';
  import Icon from '../components/Icon.svelte';
  import Poster from '../components/Poster.svelte';
  import Status from '../components/Status.svelte';
  import { api } from '../lib/api/client';
  import { app, map, PARIS, RADIUS_CHOICES, showtimeFilters } from '../lib/app.svelte';
  import { favorites } from '../lib/favorites.svelte';
  import { isCoveredPosition } from '../lib/coverage';
  import { formatRuntime, timeAgo, timeOf } from '../lib/format';
  import { router } from '../lib/router.svelte';
  import { resource } from '../lib/resource.svelte';
  import { cinemaRoute, href, movieRoute } from '../lib/routes';
  import { cityTitle, HOME_TITLE } from '../lib/seo';

  // Avec `city` (/ville/lyon), le même écran centré sur la ville, avec ses cinémas.
  let { city = null }: { city?: string | null } = $props();

  const place = resource(
    () => city,
    (slug, signal) => (slug ? api.city(slug, signal) : Promise.resolve(null)),
  );
  const cityData = $derived(place.data?.slug === city ? place.data : null);

  // Ouvrir une ville, c'est chercher autour d'elle : comme le bouton « Paris », la
  // position est gardée pour les visites suivantes.
  let centeredOn: string | null = null;
  $effect(() => {
    if (cityData && centeredOn !== cityData.slug) {
      centeredOn = cityData.slug;
      app.setPosition({ lat: cityData.lat, lng: cityData.lng }, cityData.label);
    }
  });

  /** Quitter la ville (autre position) : retour à l'accueil, l'URL ne dit plus vrai. */
  function leaveCity() {
    if (city) router.go({ name: 'home' });
  }

  const movies = resource(
    () => ({ ...showtimeFilters(), position: app.position, radius_km: app.radiusKm }),
    (args, signal) => api.nowShowing({ ...args, limit: 60 }, signal),
  );

  $effect(() => {
    map.show(cityData?.cinemas ?? []);
  });

  const isParis = $derived(app.position.lat === PARIS.lat && app.position.lng === PARIS.lng);
  const dates = $derived(app.meta?.dates_available ?? []);
</script>

<svelte:head>
  {#if !city}<title>{HOME_TITLE}</title>
  {:else if cityData}<title>{cityTitle(cityData.label, cityData.cinema_count)}</title>{/if}
</svelte:head>

{#if city && place.error}
  <Status loading={false} error={place.error} retry={place.retry} />
{/if}

<section class="intro">
  <h1>{city ? `Cinéma à ${cityData?.label ?? '…'}` : 'Séances de cinéma'}</h1>
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
    <button
      class="button"
      onclick={() => {
        app.locate();
        leaveCity();
      }}
      disabled={app.locating}
    >
      <Icon name="locate" />
      {app.locating ? 'Localisation…' : 'Autour de moi'}
    </button>
    {#if !isParis && !city}
      <button class="button" onclick={() => app.setPosition(PARIS, 'Paris')}>Paris</button>
    {/if}
  </div>
  {#if app.locateError}<p class="error">{app.locateError}</p>{/if}
</section>

{#if !isCoveredPosition(app.position)}
  <CoverageNotice what="les séances autour de vous" />
{/if}

{#if cityData}
  <section class="favorites" aria-labelledby="city-cinemas-title">
    <h2 id="city-cinemas-title" class="label">
      {cityData.cinemas.length > 1
        ? `Les ${cityData.cinemas.length} cinémas de ${cityData.name}`
        : `Le cinéma de ${cityData.name}`}
    </h2>
    <ul class="scroll-row">
      {#each cityData.cinemas as cinema (cinema.id)}
        <li class="chip">
          <a href={href(cinemaRoute(cinema), router.filters)}>{cinema.name}</a>
        </li>
      {/each}
    </ul>
  </section>
{/if}

{#if favorites.cinemas.items.length}
  <section class="favorites" aria-labelledby="favorites-title">
    <h2 id="favorites-title" class="label">Mes cinémas</h2>
    <ul class="scroll-row">
      {#each favorites.cinemas.items as cinema (cinema.id)}
        <li class="chip fav">
          <Icon name="star" size={14} filled />
          <a href={href(cinemaRoute(cinema), router.filters)}>{cinema.name}</a>
          <button
            aria-label="Retirer {cinema.name} des favoris"
            onclick={() => favorites.cinemas.remove(cinema.id)}
          >
            <Icon name="close" size={14} />
          </button>
        </li>
      {/each}
    </ul>
  </section>
{/if}

<FilterBar {dates} />

<Status loading={movies.loading && !movies.data} error={movies.error} retry={movies.retry} />

{#if movies.data}
  {#if movies.data.movies.length === 0}
    <div class="empty">
      {#if router.filters.favorites}
        <p>Aucune séance dans vos cinémas favoris avec ces critères.</p>
        <button class="button" onclick={() => router.setFilters({ favorites: false })}>
          Voir tous les cinémas
        </button>
      {:else}
        <p>Aucune séance trouvée avec ces critères.</p>
      {/if}
      {#if !router.filters.favorites && app.radiusKm < 100}
        <button class="button" onclick={() => app.setRadius(app.radiusKm < 30 ? 30 : 100)}>
          Chercher plus loin
        </button>
      {/if}
    </div>
  {:else}
    <ol class="movies" class:stale={movies.loading}>
      {#each movies.data.movies as item (item.movie.id)}
        <li>
          <a class="movie" href={href(movieRoute(item.movie), router.filters)}>
            <Poster url={item.movie.poster_url} title={item.movie.title} width={72} height={96} />
            <div class="info">
              <h2>{item.movie.title}</h2>
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

  .label {
    font-size: 0.8rem;
    font-weight: 650;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: var(--muted);
  }

  .favorites ul {
    list-style: none;
    margin: 0;
    padding: 0;
  }

  .fav {
    padding-right: 0.3em;
  }

  .fav :global(svg) {
    color: #e0a526;
    vertical-align: -1px;
  }

  .favorites .chip a {
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
