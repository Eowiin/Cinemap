<script lang="ts">
  import { app } from '../lib/app.svelte';
  import { cardBadges } from '../lib/cards';
  import { myCards } from '../lib/cards.svelte';

  /** `onlyMine` : n'affiche que mes cartes (listes) ; sinon toutes, les miennes en couleur. */
  let { cards, onlyMine = false }: { cards: string[]; onlyMine?: boolean } = $props();

  const badges = $derived(
    cardBadges(
      cards,
      app.meta?.cards ?? [],
      myCards.items.map((c) => c.id),
      onlyMine,
    ),
  );
</script>

{#each badges as { card, mine } (card.id)}
  <span class="badge" class:accent={mine} title="Carte acceptée">{card.name}</span>
{/each}
