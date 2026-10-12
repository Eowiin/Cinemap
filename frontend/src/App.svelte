<script lang="ts">
  import { onMount } from 'svelte';
  import Icon from './components/Icon.svelte';
  import MapView from './components/MapView.svelte';
  import SearchBox from './components/SearchBox.svelte';
  import ShowtimeMenu from './components/ShowtimeMenu.svelte';
  import { app, map } from './lib/app.svelte';
  import { restoreCardFilter } from './lib/cards.svelte';
  import { programme } from './lib/programme.svelte';
  import { interceptLinks, router } from './lib/router.svelte';
  import CinemaPage from './views/CinemaPage.svelte';
  import Home from './views/Home.svelte';
  import MoviePage from './views/MoviePage.svelte';
  import NotFound from './views/NotFound.svelte';
  import ProgrammePage from './views/ProgrammePage.svelte';

  // Sur mobile, la carte (MapLibre, tuiles, liste des 3 100 cinémas) ne se charge
  // que si on l'ouvre : pas de données téléchargées pour rien.
  const wide = matchMedia('(min-width: 900px)');
  let isWide = $state(wide.matches);
  const showMap = $derived(isWide || map.open);
  // Séances prévues à partir d'aujourd'hui (la veille reste en mémoire pour les séances après minuit).
  const planned = $derived(programme.items.filter((s) => s.date >= app.today).length);

  onMount(() => {
    app.loadMeta().then(() => restoreCardFilter(app.meta?.cards ?? []));
    const onChange = () => (isWide = wide.matches);
    wide.addEventListener('change', onChange);
    return () => wide.removeEventListener('change', onChange);
  });

  $effect(() => {
    if (showMap && map.all.length === 0) map.loadAll();
  });
</script>

<svelte:document onclick={interceptLinks} />

<div class="app">
  <header class="top">
    <a class="logo" href="/" aria-label="Cinemap, accueil">
      <img src="/favicon.svg" alt="" width="30" height="30" />
      <span>Cinemap</span>
    </a>
    <SearchBox />
    <a
      class="programme"
      href="/programme"
      aria-current={router.route.name === 'programme' ? 'page' : undefined}
      aria-label={planned
        ? `Mon programme, ${planned} séance${planned > 1 ? 's' : ''}`
        : 'Mon programme'}
      title="Mon programme"
    >
      <Icon name="calendar" />
      {#if planned}<span class="count">{planned}</span>{/if}
    </a>
  </header>

  <main class="panel">
    {#if router.route.name === 'home'}
      <Home />
    {:else if router.route.name === 'city'}
      {#key router.route.slug}<Home city={router.route.slug} />{/key}
    {:else if router.route.name === 'movie'}
      {#key router.route.id}<MoviePage id={router.route.id} />{/key}
    {:else if router.route.name === 'cinema'}
      {#key router.route.id}<CinemaPage id={router.route.id} />{/key}
    {:else if router.route.name === 'programme'}
      <ProgrammePage />
    {:else}
      <NotFound />
    {/if}
  </main>

  <ShowtimeMenu />

  <section class="map-area" class:open={map.open} aria-label="Carte des cinémas">
    {#if showMap}<MapView />{/if}
  </section>

  <button class="map-toggle button primary" onclick={() => (map.open = !map.open)}>
    <Icon name={map.open ? 'list' : 'map'} />
    {map.open ? 'Liste' : 'Carte'}
  </button>
</div>

<style>
  .app {
    min-height: 100dvh;
  }

  .top {
    position: sticky;
    top: 0;
    z-index: 30;
    height: var(--header-h);
    display: flex;
    align-items: center;
    gap: 1rem;
    padding: 0 1rem;
    background: color-mix(in srgb, var(--bg) 88%, transparent);
    backdrop-filter: blur(10px);
    border-bottom: 1px solid var(--border);
  }

  .logo {
    display: flex;
    align-items: center;
    gap: 0.45rem;
    font-weight: 750;
    font-size: 1.1rem;
    letter-spacing: -0.02em;
    text-decoration: none;
  }

  .programme {
    position: relative;
    display: inline-flex;
    flex: none;
    padding: 0.45rem;
    border-radius: 10px;
    color: var(--text);
  }

  .programme:hover,
  .programme[aria-current='page'] {
    background: var(--surface-2);
  }

  .count {
    position: absolute;
    top: -0.1rem;
    right: -0.2rem;
    display: grid;
    place-items: center;
    min-width: 1.15rem;
    height: 1.15rem;
    padding: 0 0.25rem;
    border-radius: 999px;
    background: var(--accent);
    color: var(--accent-text);
    font-size: 0.7rem;
    font-weight: 700;
  }

  .panel {
    padding: 1rem 1rem 6rem;
    max-width: 720px;
    margin: 0 auto;
  }

  /* Mobile : carte plein écran par-dessus la liste quand on l'ouvre. */
  .map-area {
    display: none;
    position: fixed;
    inset: var(--header-h) 0 0 0;
    z-index: 20;
  }

  .map-area.open {
    display: block;
  }

  .map-toggle {
    position: fixed;
    left: 50%;
    bottom: calc(1rem + env(safe-area-inset-bottom));
    transform: translateX(-50%);
    z-index: 25;
    box-shadow: var(--shadow);
    border-radius: 999px;
    padding: 0.6em 1.2em;
  }

  /* Bureau : liste à gauche, carte toujours visible à droite. */
  @media (min-width: 900px) {
    .logo span {
      display: inline;
    }

    .panel {
      position: fixed;
      top: var(--header-h);
      bottom: 0;
      left: 0;
      width: var(--panel-w);
      max-width: none;
      overflow-y: auto;
      padding-bottom: 2rem;
      border-right: 1px solid var(--border);
    }

    .map-area,
    .map-area.open {
      display: block;
      left: var(--panel-w);
    }

    .map-toggle {
      display: none;
    }
  }

  @media (max-width: 480px) {
    .logo span {
      display: none;
    }
  }
</style>
