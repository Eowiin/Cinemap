<script lang="ts">
  import { posterUrl } from '../lib/format';

  let {
    url,
    title,
    width,
    height,
  }: { url: string | null; title: string; width: number; height: number } = $props();

  // Image demandée en double densité pour les écrans Retina, sans dépasser l'original.
  const src = $derived(posterUrl(url, width * 2, height * 2));
</script>

<div class="poster" style:width="{width}px" style:height="{height}px">
  {#if src}
    <img {src} alt="Affiche de {title}" loading="lazy" decoding="async" {width} {height} />
  {:else}
    <span>{title}</span>
  {/if}
</div>

<style>
  .poster {
    flex: none;
    border-radius: 8px;
    overflow: hidden;
    background: var(--surface-2);
    display: grid;
    place-items: center;
  }

  img {
    width: 100%;
    height: 100%;
    object-fit: cover;
    display: block;
  }

  span {
    padding: 0.4rem;
    font-size: 0.7rem;
    text-align: center;
    color: var(--muted);
  }
</style>
