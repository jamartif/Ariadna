<script lang="ts">
  let { size = 24 }: { size?: number } = $props();
</script>

<!-- Hilo de Ariadna: espiral de laberinto que termina en un nodo, en el cian del icono. -->
<svg width={size} height={size} viewBox="0 0 32 32" fill="none" aria-hidden="true" focusable="false">
  <rect x="1" y="1" width="30" height="30" rx="8" fill="#0b1f26" />
  <path
    d="M16 26c-5.5 0-10-4.5-10-10S10.5 6 16 6s8 3.6 8 8-3.2 6.5-6.5 6.5S12 18 12 15.5 13.8 12 16 12"
    stroke="#22d3ee"
    stroke-width="2.2"
    stroke-linecap="round"
  />
  <circle cx="16" cy="12" r="2.2" fill="#06b6d4" />
</svg>
