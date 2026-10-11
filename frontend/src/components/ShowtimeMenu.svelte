<script lang="ts">
  import { dateLabel, timeOf } from '../lib/format';
  import { app } from '../lib/app.svelte';
  import { clock, conflicts, DEFAULT_RUNTIME_MIN, endMinutes, linkLabel } from '../lib/planner';
  import { programme, showtimeMenu } from '../lib/programme.svelte';
  import { router } from '../lib/router.svelte';
  import Icon from './Icon.svelte';

  // Un seul menu pour toute l'application (App.svelte), ouvert par un appui sur un horaire.

  let dialog: HTMLDialogElement;

  const stop = $derived(showtimeMenu.stop);
  const planned = $derived(stop ? programme.has(stop.id) : false);
  const issues = $derived(stop ? conflicts(programme.items, stop) : []);
  const dayCount = $derived(
    stop ? programme.items.filter((s) => s.date === stop.date && s.id !== stop.id).length : 0,
  );

  $effect(() => {
    if (stop && !dialog.open) dialog.showModal();
    else if (!stop && dialog.open) dialog.close();
  });

  function toggle() {
    if (stop) programme.toggle($state.snapshot(stop));
  }

  function openProgramme() {
    if (!stop) return;
    const date = stop.date;
    dialog.close();
    router.goto(`/programme${date === app.today ? '' : `?date=${date}`}`);
  }

  function closeOnBackdrop(event: MouseEvent) {
    if (event.target === dialog) dialog.close();
  }
</script>

<dialog
  bind:this={dialog}
  class="sheet"
  aria-labelledby="showtime-title"
  onclick={closeOnBackdrop}
  onclose={() => showtimeMenu.close()}
>
  {#if stop}
    <div class="sheet-body">
      <header>
        <div>
          <h2 id="showtime-title">{stop.movie.title}</h2>
          <p class="muted">{stop.cinema.name}</p>
        </div>
        <button class="icon" aria-label="Fermer" onclick={() => dialog.close()}>
          <Icon name="close" />
        </button>
      </header>

      <p class="when">
        <strong>{dateLabel(stop.date, app.today)} à {timeOf(stop.starts_at)}</strong>
        <span class="badge">{stop.version}</span>
        {#each showtimeMenu.formats as format (format)}<span class="badge">{format}</span>{/each}
        <br />
        <span class="muted small">
          Fin vers {clock(endMinutes(stop))}{stop.movie.runtime_min === null
            ? ` (durée inconnue : ${DEFAULT_RUNTIME_MIN / 60} h supposées)`
            : ''}, pubs comprises
        </span>
      </p>

      {#if issues.length}
        <ul class="issues">
          {#each issues as { other, link, after } (other.id)}
            <li class:warn={link.status !== 'tight'}>
              <Icon name="alert" size={16} />
              <span>
                {after ? 'Suivie de' : 'Après'} « {other.movie.title} » ({timeOf(other.starts_at)}, {other
                  .cinema.name}) : {linkLabel(link).toLowerCase()}
              </span>
            </li>
          {/each}
        </ul>
      {/if}

      <div class="actions">
        {#if stop.booking_url}
          <a class="button primary" href={stop.booking_url} target="_blank" rel="noopener">
            <Icon name="external" size={16} /> Réserver
          </a>
        {/if}
        <button class="button" class:on={planned} aria-pressed={planned} onclick={toggle}>
          <Icon name={planned ? 'check' : 'plus'} size={16} />
          {planned ? 'Dans mon programme' : 'Ajouter au programme'}
        </button>
        {#if planned || dayCount}
          <button class="link" onclick={openProgramme}>
            <Icon name="calendar" size={16} /> Voir mon programme ({dayCount + (planned ? 1 : 0)})
          </button>
        {/if}
      </div>
    </div>
  {/if}
</dialog>

<style>
  header {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 0.6rem;
  }

  h2 {
    font-size: 1.15rem;
  }

  p {
    margin: 0;
  }

  .when {
    line-height: 1.7;
  }

  .when .badge {
    margin-left: 0.3rem;
  }

  .small {
    font-size: 0.86rem;
  }

  .issues {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    gap: 0.4rem;
    font-size: 0.9rem;
  }

  .issues li {
    display: flex;
    gap: 0.45rem;
    align-items: flex-start;
    color: #a86a00;
  }

  .issues li.warn {
    color: var(--accent);
  }

  .issues :global(svg) {
    flex: none;
    margin-top: 0.15rem;
  }

  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: 0.5rem;
    align-items: center;
  }

  .on {
    border-color: var(--accent);
    background: var(--accent-soft);
    color: var(--accent);
  }

  .icon {
    display: inline-flex;
    padding: 0.35rem;
    border: none;
    border-radius: 999px;
    background: none;
  }

  .icon:hover {
    background: var(--surface-2);
  }

  .link {
    display: inline-flex;
    align-items: center;
    gap: 0.3rem;
    border: none;
    background: none;
    padding: 0.3rem 0;
    color: var(--accent);
    font-weight: 600;
  }

  @media (prefers-color-scheme: dark) {
    .issues li {
      color: #e8b04a;
    }
  }
</style>
