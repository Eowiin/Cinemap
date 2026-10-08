<script lang="ts">
  import { api, isAbort, latest } from '../lib/api/client';
  import type { SearchResults } from '../lib/api/types';
  import { router } from '../lib/router.svelte';
  import { href, type Route } from '../lib/routes';
  import Icon from './Icon.svelte';

  let query = $state('');
  let results = $state<SearchResults | null>(null);
  let open = $state(false);
  let active = $state(-1);
  const run = latest();
  let timer: ReturnType<typeof setTimeout> | undefined;

  type Item = { route: Route; label: string; detail: string | null; kind: 'Film' | 'Cinéma' };
  const items = $derived<Item[]>(
    results
      ? [
          ...results.movies.map((m) => ({
            route: { name: 'movie' as const, id: m.id },
            label: m.title,
            detail: m.release_date?.slice(0, 4) ?? null,
            kind: 'Film' as const,
          })),
          ...results.cinemas.map((c) => ({
            route: { name: 'cinema' as const, id: c.id },
            label: c.name,
            detail: c.city,
            kind: 'Cinéma' as const,
          })),
        ]
      : [],
  );

  function onInput() {
    clearTimeout(timer);
    active = -1;
    const q = query.trim();
    if (q.length < 2) {
      results = null;
      return;
    }
    // Petite attente : une requête par pause de frappe, pas une par lettre.
    timer = setTimeout(() => {
      run((signal) => api.search(q, signal)).then(
        (r) => {
          results = r;
          open = true;
        },
        (error) => {
          if (!isAbort(error)) results = { movies: [], cinemas: [] };
        },
      );
    }, 200);
  }

  function choose(item: Item) {
    open = false;
    query = '';
    results = null;
    router.go(item.route);
    (document.activeElement as HTMLElement | null)?.blur();
  }

  function onKeydown(event: KeyboardEvent) {
    if (!open || items.length === 0) return;
    if (event.key === 'ArrowDown') {
      active = (active + 1) % items.length;
      event.preventDefault();
    } else if (event.key === 'ArrowUp') {
      active = (active - 1 + items.length) % items.length;
      event.preventDefault();
    } else if (event.key === 'Enter') {
      const item = items[active === -1 ? 0 : active];
      if (item) choose(item);
      event.preventDefault();
    } else if (event.key === 'Escape') {
      open = false;
    }
  }
</script>

<div class="search" role="search">
  <label class="field">
    <Icon name="search" size={17} />
    <span class="visually-hidden">Rechercher</span>
    <input
      type="search"
      placeholder="Un film, un cinéma, une ville…"
      autocomplete="off"
      bind:value={query}
      oninput={onInput}
      onkeydown={onKeydown}
      onfocus={() => (open = !!results)}
      onblur={() => setTimeout(() => (open = false), 150)}
      role="combobox"
      aria-expanded={open && !!results}
      aria-controls="search-results"
      aria-activedescendant={active >= 0 ? `search-item-${active}` : undefined}
    />
  </label>
  {#if open && results}
    <ul class="results card" id="search-results" role="listbox">
      {#each items as item, i (item.kind + JSON.stringify(item.route))}
        <li id="search-item-{i}" role="option" aria-selected={i === active}>
          <a
            href={href(item.route, router.filters)}
            onclick={(e) => {
              e.preventDefault();
              choose(item);
            }}
          >
            <span class="badge">{item.kind}</span>
            <span class="label">{item.label}</span>
            {#if item.detail}<span class="muted detail">{item.detail}</span>{/if}
          </a>
        </li>
      {:else}
        <li class="none muted">Aucun résultat</li>
      {/each}
    </ul>
  {/if}
</div>

<style>
  .search {
    position: relative;
    flex: 1;
    max-width: 480px;
  }

  .field {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    padding: 0 0.8rem;
    height: 40px;
    border-radius: 999px;
    background: var(--surface-2);
    color: var(--muted);
  }

  input {
    flex: 1;
    min-width: 0;
    border: none;
    background: none;
    outline: none;
    color: var(--text);
  }

  .field:focus-within {
    outline: 2px solid var(--accent);
  }

  .results {
    position: absolute;
    top: calc(100% + 6px);
    left: 0;
    right: 0;
    z-index: 20;
    list-style: none;
    margin: 0;
    padding: 0.3rem;
    box-shadow: var(--shadow);
    max-height: 70vh;
    overflow-y: auto;
  }

  a {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    padding: 0.5rem 0.6rem;
    border-radius: 8px;
    text-decoration: none;
  }

  [aria-selected='true'] a,
  a:hover {
    background: var(--surface-2);
  }

  .label {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .detail {
    margin-left: auto;
    font-size: 0.85rem;
    white-space: nowrap;
  }

  .none {
    padding: 0.6rem;
  }
</style>
