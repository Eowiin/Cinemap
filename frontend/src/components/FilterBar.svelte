<script lang="ts">
  import type { VersionFilter } from '../lib/api/types';
  import { app } from '../lib/app.svelte';
  import { toggledCard } from '../lib/cards';
  import { setCardFilter } from '../lib/cards.svelte';
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

  const cards = $derived(app.meta?.cards ?? []);

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
  {#if cards.length}
    <div class="scroll-row" role="group" aria-label="Cinémas qui acceptent la carte">
      {#each cards as card (card.id)}
        <button
          class="chip"
          aria-pressed={router.filters.cards?.includes(card.id) ?? false}
          title="Seulement les cinémas qui acceptent {card.name}"
          onclick={() => setCardFilter(toggledCard(router.filters.cards, card.id))}
          >{card.name}</button
        >
      {/each}
    </div>
  {/if}
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
