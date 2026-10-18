<script lang="ts">
  // Même texte que la page servie par axum (backend/src/api/pages/about.rs) : un seul fichier.
  import about from '../content/about.html?raw';
</script>

<svelte:head><title>À propos et mentions légales · Cinemap</title></svelte:head>

<!-- eslint-disable-next-line svelte/no-at-html-tags -- texte du dépôt, pas une donnée -->
{@html about}
