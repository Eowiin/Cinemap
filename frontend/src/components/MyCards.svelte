<script lang="ts">
  import { app } from '../lib/app.svelte';
  import { myCards } from '../lib/cards.svelte';
  import { router } from '../lib/router.svelte';

  const cards = $derived(app.meta?.cards ?? []);

  function toggle(id: string) {
    myCards.toggle({ id });
    // Filtre actif : il suit mes cartes (et s'arrête si je n'en ai plus).
    if (router.filters.cards) {
      const mine = myCards.items.map((c) => c.id);
      router.setFilters({ cards: mine.length ? mine : null });
    }
  }
</script>

{#if cards.length}
  <div class="my-cards">
    <span class="muted">Mes cartes</span>
    <div class="scroll-row" role="group" aria-label="Mes cartes d'abonnement">
      {#each cards as card (card.id)}
        <button class="chip" aria-pressed={myCards.has(card.id)} onclick={() => toggle(card.id)}
          >{card.name}</button
        >
      {/each}
    </div>
  </div>
{/if}

<style>
  .my-cards {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    font-size: 0.93rem;
    min-width: 0;
  }

  .my-cards > span {
    flex: none;
  }
</style>
