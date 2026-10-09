<script lang="ts" generics="T extends Keyed">
  import type { StoredList } from '../lib/stored.svelte';
  import type { Keyed } from '../lib/stored';
  import Icon from './Icon.svelte';

  /** `compact` : l'étoile seule, pour une ligne de liste. */
  let {
    list,
    item,
    name,
    compact = false,
  }: { list: StoredList<T>; item: T; name: string; compact?: boolean } = $props();

  const on = $derived(list.has(item.id));
  const label = $derived(on ? `Retirer ${name} des favoris` : `Ajouter ${name} aux favoris`);
</script>

<button
  class={compact ? 'star' : 'button'}
  class:on
  aria-pressed={on}
  aria-label={compact ? label : undefined}
  title={label}
  onclick={() => list.toggle(item)}
>
  <Icon name="star" size={compact ? 18 : 16} filled={on} />
  {#if !compact}{on ? 'Favori' : 'Ajouter aux favoris'}{/if}
</button>

<style>
  .on :global(svg) {
    color: #e0a526;
  }

  .star {
    display: inline-flex;
    padding: 0.2rem;
    border: none;
    background: none;
    color: var(--muted);
    border-radius: 6px;
  }

  .star:hover {
    background: var(--surface-2);
  }
</style>
