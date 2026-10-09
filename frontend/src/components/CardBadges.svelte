<script lang="ts">
  import { app } from '../lib/app.svelte';
  import { cardBadges } from '../lib/cards';
  import { router } from '../lib/router.svelte';

  /** Cartes acceptées par le cinéma ; celles du filtre en couleur. */
  let { cards }: { cards: string[] } = $props();

  const badges = $derived(cardBadges(cards, app.meta?.cards ?? [], router.filters.cards));
</script>

{#each badges as { card, active } (card.id)}
  <span class="badge" class:accent={active} title="Carte acceptée">{card.name}</span>
{/each}
