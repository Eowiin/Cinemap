<script lang="ts">
  import FavoriteButton from '../components/FavoriteButton.svelte';
  import FilterBar from '../components/FilterBar.svelte';
  import Icon from '../components/Icon.svelte';
  import Poster from '../components/Poster.svelte';
  import ShowtimeChips from '../components/ShowtimeChips.svelte';
  import Status from '../components/Status.svelte';
  import { api } from '../lib/api/client';
  import { map, showtimeFilters } from '../lib/app.svelte';
  import { favoriteCinema } from '../lib/favorites';
  import { favorites } from '../lib/favorites.svelte';
  import { formatRuntime } from '../lib/format';
  import { resource } from '../lib/resource.svelte';
  import { router } from '../lib/router.svelte';
  import { href } from '../lib/routes';

  let { id }: { id: string } = $props();

  const programme = resource(
    () => ({ id, ...showtimeFilters() }),
    ({ id, ...filters }, signal) => api.cinemaShowtimes(id, filters, signal),
  );

  const data = $derived(programme.data?.cinema.id === id ? programme.data : null);
  const cinema = $derived(data?.cinema ?? null);

  $effect(() => {
    if (cinema) map.show([cinema], { lat: cinema.lat, lng: cinema.lng });
  });

  const favorite = $derived(cinema ? favoriteCinema(cinema) : null);
  $effect(() => {
    if (favorite) favorites.cinemas.refresh(favorite);
  });

  // Une adresse peut contenir des sauts de ligne (texte d'accès AlloCiné).
  const address = $derived(cinema?.address?.split('\n')[0] ?? null);
  const directions = $derived(
    cinema
      ? `https://www.google.com/maps/dir/?api=1&destination=${cinema.lat},${cinema.lng}`
      : null,
  );
</script>

<svelte:head><title>{cinema ? `${cinema.name} · Cinemap` : 'Cinemap'}</title></svelte:head>

<a class="back" href={href({ name: 'home' }, router.filters)}><Icon name="back" /> À l'affiche</a>

<Status loading={programme.loading && !data} error={programme.error} retry={programme.retry} />

{#if data && cinema}
  <header>
    <h1>{cinema.name}</h1>
    {#if address}<p class="muted">{address}</p>{/if}
    <p class="badges">
      {#if cinema.art_et_essai}<span class="badge accent">Art et Essai</span>{/if}
      {#if cinema.screens}<span class="badge"
          >{cinema.screens} salle{cinema.screens > 1 ? 's' : ''}</span
        >{/if}
      {#if cinema.seats}<span class="badge">{cinema.seats.toLocaleString('fr-FR')} fauteuils</span
        >{/if}
    </p>
    <p class="links">
      {#if favorite}
        <FavoriteButton list={favorites.cinemas} item={favorite} name={favorite.name} />
      {/if}
      {#if directions}
        <a class="button" href={directions} target="_blank" rel="noopener"
          ><Icon name="route" size={16} /> Itinéraire</a
        >
      {/if}
      <a class="button" href={cinema.allocine_url} target="_blank" rel="noopener"
        ><Icon name="external" size={16} /> AlloCiné</a
      >
    </p>
  </header>

  <section class="sessions">
    <FilterBar dates={data.dates} />
    {#if data.movies.length === 0}
      <p class="empty muted">Aucune séance avec ces critères.</p>
    {:else}
      <ul class="movies" class:stale={programme.loading}>
        {#each data.movies as { movie, showtimes } (movie.id)}
          <li class="movie">
            <a href={href({ name: 'movie', id: movie.id }, router.filters)} tabindex="-1">
              <Poster url={movie.poster_url} title={movie.title} width={56} height={75} />
            </a>
            <div class="info">
              <a class="title" href={href({ name: 'movie', id: movie.id }, router.filters)}
                >{movie.title}</a
              >
              <p class="muted small">
                {[movie.genres.slice(0, 2).join(', '), formatRuntime(movie.runtime_min)]
                  .filter(Boolean)
                  .join(' · ')}
              </p>
              <ShowtimeChips {showtimes} date={data.date} />
            </div>
          </li>
        {/each}
      </ul>
    {/if}
  </section>
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
    gap: 0.4rem;
  }

  h1 {
    font-size: 1.5rem;
    letter-spacing: -0.02em;
  }

  p {
    margin: 0;
  }

  .badges,
  .links {
    display: flex;
    flex-wrap: wrap;
    gap: 0.4rem;
  }

  .links {
    margin-top: 0.3rem;
  }

  .sessions {
    margin-top: 1.4rem;
    display: grid;
    gap: 0.8rem;
  }

  .movies {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    gap: 1.1rem;
    transition: opacity 0.15s;
  }

  .stale {
    opacity: 0.55;
  }

  .movie {
    display: flex;
    gap: 0.8rem;
  }

  .info {
    min-width: 0;
    display: grid;
    gap: 0.35rem;
    align-content: start;
  }

  .title {
    font-weight: 650;
    text-decoration: none;
  }

  .title:hover {
    text-decoration: underline;
  }

  .small {
    font-size: 0.86rem;
  }

  .empty {
    padding: 1rem 0;
  }
</style>
