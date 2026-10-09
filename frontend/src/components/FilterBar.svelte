<script lang="ts">
  import type { VersionFilter } from '../lib/api/types';
  import { app } from '../lib/app.svelte';
  import { myCards } from '../lib/cards.svelte';
  import { dateLabel } from '../lib/format';
  import { router } from '../lib/router.svelte';

  /** Jours proposés (ceux qui ont des séances) ; le jour choisi et aujourd'hui sont toujours affichés. */
  let { dates }: { dates: string[] } = $props();

  const selected = $derived(router.filters.date ?? app.today);
  const days = $derived(
    [...new Set([app.today, selected, ...dates])].filter((d) => d >= app.today).sort(),
  );

  const versions: { value: VersionFilter | null; label: string }[] = [
    { value: null, label: 'Toutes' },
    { value: 'VF', label: 'VF' },
    { value: 'VO', label: 'VO / VOST' },
  ];
  const afters = ['18:00', '20:00', '22:00'];

  // Filtre « mes cartes » : proposé si j'ai choisi des cartes, ou si un lien partagé en filtre.
  const mine = $derived(myCards.items.map((c) => c.id));
  const cardFilter = $derived(router.filters.cards);
  const cardLabel = $derived.by(() => {
    const ids = cardFilter ?? mine;
    const sameAsMine = ids.length === mine.length && ids.every((id) => mine.includes(id));
    if (sameAsMine) return mine.length > 1 ? 'Mes cartes' : 'Ma carte';
    const names = app.meta?.cards.filter((c) => ids.includes(c.id)).map((c) => c.name) ?? [];
    return names.join(', ') || 'Cartes';
  });

  function pickDate(date: string) {
    // `null` = aujourd'hui : un lien partagé sans date reste valable demain.
    router.setFilters({ date: date === app.today ? null : date });
  }
</script>

<div class="filters">
  <div class="scroll-row" role="group" aria-label="Jour">
    {#each days as day (day)}
      <button
        class="chip"
        aria-pressed={day === selected}
        disabled={day !== selected && day !== app.today && !dates.includes(day)}
        onclick={() => pickDate(day)}>{dateLabel(day, app.today)}</button
      >
    {/each}
  </div>
  <div class="row">
    <div class="scroll-row" role="group" aria-label="Version">
      {#each versions as v (v.label)}
        <button
          class="chip"
          aria-pressed={router.filters.version === v.value}
          onclick={() => router.setFilters({ version: v.value })}>{v.label}</button
        >
      {/each}
    </div>
    {#if mine.length || cardFilter}
      <button
        class="chip"
        aria-pressed={cardFilter !== null}
        title="Seulement les cinémas qui acceptent mes cartes d'abonnement"
        onclick={() => router.setFilters({ cards: cardFilter ? null : mine })}>{cardLabel}</button
      >
    {/if}
    <label class="after">
      <span class="visually-hidden">Heure</span>
      <select
        value={router.filters.after ?? ''}
        onchange={(e) => router.setFilters({ after: e.currentTarget.value || null })}
      >
        <option value=""
          >{selected === app.today ? 'À partir de maintenant' : 'Toute la journée'}</option
        >
        {#each afters as after (after)}
          <option value={after}>Après {after.replace(':00', ' h')}</option>
        {/each}
      </select>
    </label>
  </div>
</div>

<style>
  .filters {
    display: grid;
    gap: 0.5rem;
  }

  .row {
    display: flex;
    gap: 0.5rem;
    align-items: center;
    justify-content: space-between;
    flex-wrap: wrap;
  }

  select {
    padding: 0.35em 0.6em;
    border-radius: 999px;
    border: 1px solid var(--border);
    background: var(--surface);
    font-size: 0.93rem;
  }
</style>
