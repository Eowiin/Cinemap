<script lang="ts">
  import type {
    GeoJSONSource,
    LngLatBoundsLike,
    Map as MlMap,
    MapLayerMouseEvent,
  } from 'maplibre-gl';
  import type { FeatureCollection, Point } from 'geojson';
  import { onMount } from 'svelte';
  import type { Position } from '../lib/api/client';
  import type { CinemaSummary } from '../lib/api/types';
  import { app, map } from '../lib/app.svelte';
  import { cardBadges } from '../lib/cards';
  import { router } from '../lib/router.svelte';

  const STYLES = {
    light: 'https://tiles.openfreemap.org/styles/positron',
    dark: 'https://tiles.openfreemap.org/styles/dark',
  };
  const KM_PER_LAT_DEGREE = 111.32;

  let container: HTMLDivElement;
  let instance = $state<MlMap | null>(null);
  /** Incrémenté à chaque (re)chargement du style : les sources sont à recréer. */
  let styleVersion = $state(0);

  const dark = matchMedia('(prefers-color-scheme: dark)');

  /**
   * Carte impossible à afficher : un message (et le détail technique, à nous envoyer)
   * au lieu d'un fond gris muet. Cas vus : WebGL 2 absent ou bloqué (MapLibre 6 en a
   * besoin), module ou fond de carte qui ne se charge pas.
   */
  let failure = $state<{ message: string; detail: string } | null>(null);

  function fail(message: string, error: unknown) {
    const detail = error instanceof Error ? error.message : String(error);
    failure = { message, detail: `${detail} · ${gpuName()} · ${navigator.userAgent}` };
    console.error('[carte]', message, error);
  }

  /** Carte graphique vue par WebGL : à nous envoyer avec le message d'erreur. */
  function gpuName(): string {
    try {
      const gl = document.createElement('canvas').getContext('webgl2');
      const info = gl?.getExtension('WEBGL_debug_renderer_info');
      return (info && gl ? String(gl.getParameter(info.UNMASKED_RENDERER_WEBGL)) : null) ?? '?';
    } catch {
      return '?';
    }
  }

  function hasWebGl2(): boolean {
    try {
      return document.createElement('canvas').getContext('webgl2') !== null;
    } catch {
      return false;
    }
  }

  function features(cinemas: CinemaSummary[]): FeatureCollection<Point> {
    return {
      type: 'FeatureCollection',
      features: cinemas.map((c) => ({
        type: 'Feature',
        geometry: { type: 'Point', coordinates: [c.lng, c.lat] },
        properties: { id: c.id, name: c.name },
      })),
    };
  }

  /** Nom du cinéma suivi des cartes qu'il accepte (« UGC Les Halles · UGC Illimité »). */
  function popupText(id: string, name: string): string {
    const cinema = map.highlighted.find((c) => c.id === id) ?? map.all.find((c) => c.id === id);
    const badges = cardBadges(cinema?.cards ?? [], app.meta?.cards ?? [], null);
    return [name, ...badges.map((badge) => badge.card.name)].join(' · ');
  }

  function point(p: Position): FeatureCollection<Point> {
    return {
      type: 'FeatureCollection',
      features: [
        {
          type: 'Feature',
          geometry: { type: 'Point', coordinates: [p.lng, p.lat] },
          properties: {},
        },
      ],
    };
  }

  function radiusBounds(p: Position, km: number): LngLatBoundsLike {
    const dlat = km / KM_PER_LAT_DEGREE;
    const dlng = km / (KM_PER_LAT_DEGREE * Math.cos((p.lat * Math.PI) / 180));
    return [
      [p.lng - dlng, p.lat - dlat],
      [p.lng + dlng, p.lat + dlat],
    ];
  }

  function addLayers(m: MlMap) {
    const css = getComputedStyle(document.documentElement);
    const accent = css.getPropertyValue('--accent').trim();
    const surface = css.getPropertyValue('--surface').trim();
    const text = css.getPropertyValue('--text').trim();
    const muted = css.getPropertyValue('--muted').trim();
    const font = ['Noto Sans Bold'];

    m.addSource('cinemas', {
      type: 'geojson',
      data: features([]),
      cluster: true,
      clusterRadius: 42,
      clusterMaxZoom: 11,
    });
    m.addSource('highlight', { type: 'geojson', data: features([]) });
    m.addSource('position', { type: 'geojson', data: point(app.position) });

    m.addLayer({
      id: 'clusters',
      type: 'circle',
      source: 'cinemas',
      filter: ['has', 'point_count'],
      paint: {
        'circle-color': surface,
        'circle-stroke-color': muted,
        'circle-stroke-width': 1,
        'circle-radius': ['step', ['get', 'point_count'], 13, 20, 17, 100, 22],
      },
    });
    m.addLayer({
      id: 'cluster-count',
      type: 'symbol',
      source: 'cinemas',
      filter: ['has', 'point_count'],
      layout: { 'text-field': '{point_count_abbreviated}', 'text-font': font, 'text-size': 12 },
      paint: { 'text-color': text },
    });
    m.addLayer({
      id: 'cinema',
      type: 'circle',
      source: 'cinemas',
      filter: ['!', ['has', 'point_count']],
      paint: {
        'circle-color': muted,
        'circle-radius': 5,
        'circle-stroke-color': surface,
        'circle-stroke-width': 1.5,
      },
    });
    m.addLayer({
      id: 'highlight',
      type: 'circle',
      source: 'highlight',
      paint: {
        'circle-color': accent,
        'circle-radius': 8,
        'circle-stroke-color': surface,
        'circle-stroke-width': 2,
      },
    });
    m.addLayer({
      id: 'highlight-label',
      type: 'symbol',
      source: 'highlight',
      minzoom: 11,
      layout: {
        'text-field': ['get', 'name'],
        'text-font': font,
        'text-size': 12,
        'text-offset': [0, 1.1],
        'text-anchor': 'top',
        'text-optional': true,
      },
      paint: { 'text-color': text, 'text-halo-color': surface, 'text-halo-width': 1.5 },
    });
    m.addLayer({
      id: 'position',
      type: 'circle',
      source: 'position',
      paint: {
        'circle-color': '#2b7de9',
        'circle-radius': 7,
        'circle-stroke-color': '#ffffff',
        'circle-stroke-width': 2.5,
      },
    });
    styleVersion++;
  }

  onMount(() => {
    let disposed = false;
    let removeThemeListener = () => {};

    if (!hasWebGl2()) {
      fail(
        "Votre navigateur n'active pas WebGL 2, nécessaire à la carte. Mettez Chrome à jour ou activez l'accélération matérielle.",
        'getContext("webgl2") = null',
      );
      return;
    }

    Promise.all([
      import('maplibre-gl'),
      import('maplibre-gl/dist/maplibre-gl-worker.mjs?url'),
      import('maplibre-gl/dist/maplibre-gl.css'),
    ])
      .then(([maplibregl, { default: workerUrl }]) => {
        if (disposed) return;
        // MapLibre 6 charge son worker à part : Vite doit en publier le fichier.
        maplibregl.setWorkerUrl(workerUrl);
        let m: MlMap;
        try {
          m = new maplibregl.Map({
            container,
            style: dark.matches ? STYLES.dark : STYLES.light,
            center: [app.position.lng, app.position.lat],
            zoom: 11,
            attributionControl: { compact: true },
          });
        } catch (error) {
          fail("La carte n'a pas pu démarrer sur ce navigateur.", error);
          return;
        }
        m.addControl(new maplibregl.NavigationControl({ showCompass: false }), 'top-right');
        m.on('style.load', () => addLayers(m));
        // Rien d'affiché au bout de 20 s (fond de carte bloqué par un bloqueur de pub ou un
        // « DNS privé », réseau très lent) : on le dit au lieu de laisser un fond vide.
        const watchdog = setTimeout(() => {
          if (!disposed && styleVersion === 0) {
            fail(
              "Le fond de carte ne se charge pas. Un bloqueur de publicité ou le « DNS privé » d'Android bloque peut-être tiles.openfreemap.org.",
              'style non chargé après 20 s',
            );
          }
        }, 20_000);
        m.once('style.load', () => clearTimeout(watchdog));
        // Une tuile en erreur passe ; un fond de carte qui ne charge jamais, non.
        m.on('error', (event) => {
          if (styleVersion === 0)
            fail("Le fond de carte (OpenFreeMap) n'a pas pu se charger.", event.error);
        });

        const popup = new maplibregl.Popup({ closeButton: false, closeOnClick: false, offset: 10 });
        for (const layer of ['cinema', 'highlight']) {
          m.on('click', layer, (e: MapLayerMouseEvent) => {
            const id = e.features?.[0]?.properties?.id as string | undefined;
            if (!id) return;
            popup.remove();
            map.open = false;
            router.go({ name: 'cinema', id });
          });
          m.on('mouseenter', layer, (e: MapLayerMouseEvent) => {
            m.getCanvas().style.cursor = 'pointer';
            const f = e.features?.[0];
            if (f?.geometry.type === 'Point') {
              popup
                .setLngLat(f.geometry.coordinates as [number, number])
                .setText(
                  popupText(String(f.properties?.id ?? ''), String(f.properties?.name ?? '')),
                )
                .addTo(m);
            }
          });
          m.on('mouseleave', layer, () => {
            m.getCanvas().style.cursor = '';
            popup.remove();
          });
        }
        m.on('click', 'clusters', async (e: MapLayerMouseEvent) => {
          const f = e.features?.[0];
          if (!f || f.geometry.type !== 'Point') return;
          const source = m.getSource('cinemas') as GeoJSONSource;
          const zoom = await source.getClusterExpansionZoom(f.properties?.cluster_id as number);
          m.easeTo({ center: f.geometry.coordinates as [number, number], zoom });
        });
        m.on('mouseenter', 'clusters', () => (m.getCanvas().style.cursor = 'pointer'));
        m.on('mouseleave', 'clusters', () => (m.getCanvas().style.cursor = ''));

        const onTheme = () =>
          m.setStyle(dark.matches ? STYLES.dark : STYLES.light, { diff: false });
        dark.addEventListener('change', onTheme);
        removeThemeListener = () => dark.removeEventListener('change', onTheme);
        instance = m;
      })
      .catch((error) => fail("La carte n'a pas pu se charger. Vérifiez la connexion.", error));

    return () => {
      disposed = true;
      removeThemeListener();
      instance?.remove();
      instance = null;
    };
  });

  $effect(() => {
    void styleVersion;
    (instance?.getSource('cinemas') as GeoJSONSource | undefined)?.setData(features(map.all));
  });

  $effect(() => {
    void styleVersion;
    (instance?.getSource('position') as GeoJSONSource | undefined)?.setData(point(app.position));
  });

  $effect(() => {
    void styleVersion;
    (instance?.getSource('highlight') as GeoJSONSource | undefined)?.setData(
      features(map.highlighted),
    );
  });

  // Cadrage : le cinéma affiché, sinon les cinémas mis en avant et la position, sinon le rayon.
  $effect(() => {
    const m = instance;
    if (!m) return;
    const focus = map.focus;
    const highlighted = map.highlighted;
    const position = app.position;
    const radius = app.radiusKm;
    if (focus) {
      m.easeTo({ center: [focus.lng, focus.lat], zoom: Math.max(m.getZoom(), 14) });
      return;
    }
    if (highlighted.length > 0) {
      const lngs = [position.lng, ...highlighted.map((c) => c.lng)];
      const lats = [position.lat, ...highlighted.map((c) => c.lat)];
      m.fitBounds(
        [
          [Math.min(...lngs), Math.min(...lats)],
          [Math.max(...lngs), Math.max(...lats)],
        ],
        { padding: 50, maxZoom: 14 },
      );
      return;
    }
    m.fitBounds(radiusBounds(position, radius), { padding: 20 });
  });
</script>

<div class="map" bind:this={container}>
  {#if failure}
    <div class="failure" role="alert">
      <p>{failure.message}</p>
      <p class="muted detail">Détail : {failure.detail}</p>
      <button class="button" onclick={() => location.reload()}>Réessayer</button>
    </div>
  {/if}
</div>

<style>
  .map {
    position: absolute;
    inset: 0;
    background: var(--surface-2);
  }

  .failure {
    position: absolute;
    inset: 0;
    z-index: 1;
    display: grid;
    align-content: center;
    justify-items: center;
    gap: 0.6rem;
    padding: 1.5rem;
    text-align: center;
    background: var(--surface-2);
  }

  .failure p {
    margin: 0;
    max-width: 32rem;
  }

  .detail {
    font-size: 0.8rem;
    overflow-wrap: anywhere;
  }

  /* Préfixé par .map : le CSS de MapLibre, chargé après le nôtre, gagnerait sinon
     (fond blanc sous un texte clair en mode sombre). */
  .map :global(.maplibregl-popup-content) {
    background: var(--surface);
    color: var(--text);
    font: 600 0.85rem system-ui;
    padding: 0.35rem 0.6rem;
    border-radius: 8px;
    box-shadow: var(--shadow);
  }

  .map :global(.maplibregl-popup-tip) {
    display: none;
  }
</style>
