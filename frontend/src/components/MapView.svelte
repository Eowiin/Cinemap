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

    Promise.all([
      import('maplibre-gl'),
      import('maplibre-gl/dist/maplibre-gl-worker.mjs?url'),
      import('maplibre-gl/dist/maplibre-gl.css'),
    ]).then(([maplibregl, { default: workerUrl }]) => {
      if (disposed) return;
      // MapLibre 6 charge son worker à part : Vite doit en publier le fichier.
      maplibregl.setWorkerUrl(workerUrl);
      const m = new maplibregl.Map({
        container,
        style: dark.matches ? STYLES.dark : STYLES.light,
        center: [app.position.lng, app.position.lat],
        zoom: 11,
        attributionControl: { compact: true },
      });
      m.addControl(new maplibregl.NavigationControl({ showCompass: false }), 'top-right');
      m.on('style.load', () => addLayers(m));

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
              .setText(String(f.properties?.name ?? ''))
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

      const onTheme = () => m.setStyle(dark.matches ? STYLES.dark : STYLES.light, { diff: false });
      dark.addEventListener('change', onTheme);
      removeThemeListener = () => dark.removeEventListener('change', onTheme);
      instance = m;
    });

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

<div class="map" bind:this={container}></div>

<style>
  .map {
    position: absolute;
    inset: 0;
    background: var(--surface-2);
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
