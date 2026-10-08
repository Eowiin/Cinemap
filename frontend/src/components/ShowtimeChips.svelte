<script lang="ts">
  import type { Showtime } from '../lib/api/types';
  import { isAfterMidnight, timeOf } from '../lib/format';

  let { showtimes, date }: { showtimes: Showtime[]; date: string } = $props();

  const label = (s: Showtime) =>
    [timeOf(s.starts_at), s.version, ...s.formats].join(' ') +
    (isAfterMidnight(s.starts_at, date) ? ' (après minuit)' : '');
</script>

<ul class="showtimes">
  {#each showtimes as s (s.id)}
    <li>
      <svelte:element
        this={s.booking_url ? 'a' : 'span'}
        class="showtime"
        class:bookable={!!s.booking_url}
        href={s.booking_url ?? undefined}
        target={s.booking_url ? '_blank' : undefined}
        rel={s.booking_url ? 'noopener' : undefined}
        title={s.booking_url ? `Réserver : ${label(s)}` : label(s)}
      >
        <strong>
          {timeOf(s.starts_at)}{#if isAfterMidnight(s.starts_at, date)}<sup>+1</sup>{/if}
        </strong>
        <span class="version" class:vf={s.version === 'VF'}>{s.version}</span>
        {#each s.formats as format (format)}<span class="format">{format}</span>{/each}
      </svelte:element>
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

  .bookable:hover {
    border-color: var(--accent);
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
