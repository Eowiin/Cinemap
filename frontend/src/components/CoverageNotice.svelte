<script lang="ts">
  import { COVERAGE } from '../lib/coverage';

  /** `what` : ce qui manque (« les séances de ce cinéma », « les séances autour de vous »). */
  let { what = 'les séances' }: { what?: string } = $props();
</script>

<p class="notice">
  Pour l'instant, Cinemap ne met à jour que les séances de l'<strong>{COVERAGE.label}</strong> :
  {what} peuvent manquer ici, c'est normal. Les autres régions arriveront plus tard.
</p>

<style>
  .notice {
    margin: 0;
    padding: 0.7rem 0.85rem;
    border-radius: var(--radius);
    background: var(--surface-2);
    font-size: 0.9rem;
  }
</style>
