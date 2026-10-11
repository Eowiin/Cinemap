<script lang="ts">
  import type { MovieSummary, Showtime } from '../lib/api/types';
  import { isAfterMidnight, timeOf } from '../lib/format';
  import { toStop, type PlanStop } from '../lib/planner';
  import { programme, showtimeMenu } from '../lib/programme.svelte';

  /** Un appui ouvre le menu de la séance (réserver, ajouter au programme). */
  let {
    showtimes,
    date,
    movie,
    cinema,
  }: {
    showtimes: Showtime[];
    date: string;
    movie: Pick<MovieSummary, 'id' | 'title' | 'runtime_min'>;
    cinema: PlanStop['cinema'];
  } = $props();

  const label = (s: Showtime) =>
    [timeOf(s.starts_at), s.version, ...s.formats].join(' ') +
    (isAfterMidnight(s.starts_at, date) ? ' (après minuit)' : '');

  const open = (s: Showtime) => showtimeMenu.open(toStop(date, s, movie, cinema), s.formats);
</script>

<ul class="showtimes">
  {#each showtimes as s (s.id)}
    {@const planned = programme.has(s.id)}
    <li>
      <button
        class="showtime"
        class:planned
        aria-haspopup="dialog"
        title={planned ? `${label(s)}, dans mon programme` : label(s)}
        onclick={() => open(s)}
      >
        <strong>
          {timeOf(s.starts_at)}{#if isAfterMidnight(s.starts_at, date)}<sup>+1</sup>{/if}
        </strong>
        <span class="version" class:vf={s.version === 'VF'}>{s.version}</span>
        {#each s.formats as format (format)}<span class="format">{format}</span>{/each}
      </button>
    </li>
  {/each}
</ul>

<style>
  .showtimes {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-wrap: wrap;
    gap: 0.4rem;
  }

  .showtime {
    display: inline-flex;
    align-items: baseline;
    gap: 0.35rem;
    padding: 0.3rem 0.55rem;
    border-radius: 8px;
    border: 1px solid var(--border);
    background: var(--surface);
    text-decoration: none;
    font-variant-numeric: tabular-nums;
  }

  .showtime:hover {
    border-color: var(--accent);
  }

  /* Dans mon programme. */
  .planned {
    border-color: var(--accent);
    background: var(--accent-soft);
  }

  strong {
    font-weight: 650;
  }

  sup {
    font-size: 0.65em;
    color: var(--muted);
  }

  .version,
  .format {
    font-size: 0.72rem;
    font-weight: 600;
    color: var(--accent);
  }

  .version.vf {
    color: var(--muted);
  }

  .format {
    color: var(--ok);
  }
</style>
