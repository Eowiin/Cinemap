<script lang="ts">
  import type { Watch } from '../lib/api/types';
  import { longDate } from '../lib/format';
  import { watchRows } from '../lib/watch';
  import Icon from './Icon.svelte';

  let { watch, lastShowtime }: { watch: Watch | null; lastShowtime: string | null } = $props();

  /** Au-delà, la ligne se replie derrière « +N » : la location aligne souvent 12 plateformes. */
  const VISIBLE = 5;

  const rows = $derived(watch ? watchRows(watch) : []);
  let expanded = $state<string[]>([]);
</script>

<section class="off-screen">
  <h2>Plus à l'affiche</h2>
  {#if lastShowtime}
    <p class="muted">Dernière séance repérée le {longDate(lastShowtime)}.</p>
  {/if}

  {#if rows.length}
    <h3>Où le voir maintenant</h3>
    <dl>
      {#each rows as row (row.label)}
        {@const open = expanded.includes(row.label)}
        {@const shown = open ? row.providers : row.providers.slice(0, VISIBLE)}
        <dt class="muted">{row.label}</dt>
        <dd>
          {#each shown as provider (provider.name)}
            <span class="provider">
              {#if provider.logo_url}
                <img src={provider.logo_url} alt="" width="22" height="22" loading="lazy" />
              {/if}
              {provider.name}
            </span>
          {/each}
          {#if !open && row.providers.length > VISIBLE}
            <button class="more" onclick={() => expanded.push(row.label)}>
              +{row.providers.length - VISIBLE}
            </button>
          {/if}
        </dd>
      {/each}
    </dl>
  {:else if watch}
    <p>Pas disponible en streaming en France pour l'instant.</p>
  {/if}

  {#if watch}
    <p class="credit muted small">
      Offres en France fournies par JustWatch · vérifié le {longDate(watch.checked_at)}
      {#if watch.link}
        · <a href={watch.link} target="_blank" rel="noopener"
          >Voir les offres <Icon name="external" size={13} /></a
        >
      {/if}
    </p>
  {/if}
</section>

<style>
  .off-screen {
    margin-top: 1.6rem;
    display: grid;
    gap: 0.6rem;
  }

  h2 {
    font-size: 1.15rem;
  }

  h3 {
    font-size: 1rem;
    margin-top: 0.4rem;
  }

  p {
    margin: 0;
  }

  dl {
    margin: 0;
    display: grid;
    grid-template-columns: auto 1fr;
    gap: 0.6rem 0.9rem;
    align-items: baseline;
  }

  dt {
    font-size: 0.9rem;
  }

  dd {
    margin: 0;
    display: flex;
    flex-wrap: wrap;
    gap: 0.35rem;
  }

  .provider,
  .more {
    display: inline-flex;
    align-items: center;
    gap: 0.35rem;
    padding: 0.2rem 0.55rem 0.2rem 0.25rem;
    border: 1px solid var(--border);
    border-radius: 999px;
    background: var(--surface);
    font-size: 0.86rem;
    line-height: 1.3;
  }

  .more {
    padding: 0.2rem 0.6rem;
    color: var(--accent);
    font-weight: 600;
  }

  img {
    border-radius: 6px;
  }

  .small {
    font-size: 0.82rem;
  }

  .credit a {
    color: inherit;
  }

  .credit :global(svg) {
    vertical-align: -2px;
  }

  @media (max-width: 420px) {
    dl {
      grid-template-columns: 1fr;
      gap: 0.25rem;
    }

    dd + dt {
      margin-top: 0.4rem;
    }
  }
</style>
