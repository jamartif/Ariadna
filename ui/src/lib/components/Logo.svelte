<script module lang="ts">
  // Contador en el ámbito del **módulo**: dentro de la instancia siempre valdría 1 y seguiría
  // habiendo ids repetidos con varios logos en la misma página.
  let nextBgId = 0;

  /** Id único del degradado de fondo para cada instancia de `Logo`. */
  export function logoBgId(): string {
    nextBgId += 1;
    return `ariadna-bg-${nextBgId}`;
  }
</script>

<script lang="ts">
  let { size = 24 }: { size?: number } = $props();

  // Único por instancia: con varios logos en la misma página `url(#ariadna-bg)` apuntaría al
  // degradado de otro componente.
  const bgId = logoBgId();
</script>

<!--
  Corona de Ariadne (Corona Borealis) en oro, con el hilo cian que la atraviesa: el hilo que
  Ariadna le dio a Teseo para salir del laberinto. Formas gruesas y tres colores para que se
  reconozca también a 16 px (los PNG/ICO/ICNS de `src-tauri/icons` salen del mismo dibujo).
-->
<svg width={size} height={size} viewBox="0 0 32 32" fill="none" aria-hidden="true" focusable="false">
  <defs>
    <linearGradient id={bgId} x1="0" y1="0" x2="0" y2="32" gradientUnits="userSpaceOnUse">
      <stop offset="0" stop-color="#0e1d2d" />
      <stop offset="1" stop-color="#07101a" />
    </linearGradient>
  </defs>
  <rect x="0" y="0" width="32" height="32" rx="7" fill="url(#{bgId})" />
  <!-- El hilo: por detrás de la corona y un tramo por delante, para que se vea que la atraviesa. -->
  <path d="M1.5 17.6c3.6 1.2 5.2 1.6 8.3 1.2" stroke="#22d3ee" stroke-width="2" stroke-linecap="round" />
  <path d="M22.2 18.8c3.1-.4 4.7-.8 8.3-2" stroke="#22d3ee" stroke-width="2" stroke-linecap="round" />
  <!-- Corona: base y tres puntas. -->
  <path d="M7.9 21.6v-3.4l1.9-7.1 3.6 4 2.6-7 2.6 7 3.6-4 1.9 7.1v3.4z" fill="#f7c554" />
  <rect x="7.6" y="19.6" width="16.8" height="3.2" rx="1.4" fill="#f7c554" />
  <rect x="7.6" y="21.5" width="16.8" height="1.3" rx="0.65" fill="#ce9628" />
  <!-- Joyas en las puntas. -->
  <circle cx="9.8" cy="9.9" r="1.5" fill="#f7c554" />
  <circle cx="9.8" cy="9.9" r="0.75" fill="#22d3ee" />
  <circle cx="16" cy="7.4" r="1.7" fill="#f7c554" />
  <circle cx="16" cy="7.4" r="0.85" fill="#22d3ee" />
  <circle cx="22.2" cy="9.9" r="1.5" fill="#f7c554" />
  <circle cx="22.2" cy="9.9" r="0.75" fill="#22d3ee" />
  <!-- Tramo del hilo por delante del centro de la corona. -->
  <path d="M13.6 18.2c1.6.3 3.2.3 4.8 0" stroke="#22d3ee" stroke-width="2.1" stroke-linecap="round" />
</svg>
