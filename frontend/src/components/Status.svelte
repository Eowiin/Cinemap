<script lang="ts">
  import { ApiError } from '../lib/api/client';

  let {
    loading = false,
    error = null,
    retry,
  }: { loading?: boolean; error?: unknown; retry?: () => void } = $props();

  const message = $derived(
    error instanceof ApiError ? error.message : error ? 'Une erreur est survenue.' : null,
  );
</script>

{#if loading}
  <div class="status" role="status">
    <span class="spinner" aria-hidden="true"></span> Chargement…
  </div>
{:else if message}
  <div class="status error" role="alert">
    <p>{message}</p>
    {#if retry}<button class="button" onclick={retry}>Réessayer</button>{/if}
  </div>
{/if}

<style>
  .status {
    display: flex;
    align-items: center;
    gap: 0.6rem;
    padding: 1.5rem 0;
    color: var(--muted);
  }

  .error {
    flex-direction: column;
    align-items: flex-start;
  }

  .error p {
    margin: 0;
    color: var(--text);
  }

  .spinner {
    width: 16px;
    height: 16px;
    border-radius: 50%;
    border: 2px solid var(--border);
    border-top-color: var(--accent);
    animation: spin 0.8s linear infinite;
  }

  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }
</style>
