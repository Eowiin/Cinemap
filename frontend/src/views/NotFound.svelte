<script lang="ts">
  import { app } from '../lib/app.svelte';
</script>

<h1>Page introuvable</h1>
<p class="muted">Ce lien ne mène nulle part.</p>
<p><a class="button primary" href="/">Voir ce qui passe autour de {app.positionLabel}</a></p>

<style>
  h1 {
    font-size: 1.5rem;
    margin-bottom: 0.5rem;
  }
</style>
