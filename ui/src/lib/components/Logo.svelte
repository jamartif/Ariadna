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
  Monograma «A» hecho con el hilo de Ariadna (el que le dio a Teseo para salir del laberinto) y
  con nodos de commit: el hilo entra por el anillo de abajo a la izquierda, sube hasta el
  vértice, baja y sale por el nodo dorado; la barra es una rama. Trazos gruesos y tres colores
  para que se lea a 16 px. `scripts/icono.py` repite este dibujo para los PNG/ICO/ICNS.
-->
<svg width={size} height={size} viewBox="0 0 32 32" fill="none" aria-hidden="true" focusable="false">
  <defs>
    <linearGradient id={bgId} x1="0" y1="0" x2="0" y2="32" gradientUnits="userSpaceOnUse">
      <stop offset="0" stop-color="#0e1d2d" />
      <stop offset="1" stop-color="#07101a" />
    </linearGradient>
  </defs>
  <rect x="0" y="0" width="32" height="32" rx="7" fill="url(#{bgId})" />
  <!-- El hilo: las dos patas de la «A» con el vértice redondeado. -->
  <path d="M3.5 27 L14.1 9.3 Q16 6 17.9 9.3 L28.5 27" stroke="#22d3ee" stroke-width="2.9" stroke-linecap="round" stroke-linejoin="round" />
  <!-- La rama que une las dos patas. -->
  <path d="M10 19 H22" stroke="#22d3ee" stroke-width="2.4" stroke-linecap="round" />
  <!-- Nodos de commit: vértice, empalmes de la rama, entrada (anillo) y salida (oro). -->
  <circle cx="16" cy="7.4" r="3" fill="#f7c554" />
  <circle cx="16" cy="7.4" r="1.1" fill="#0b1826" />
  <circle cx="10" cy="19" r="1.8" fill="#f7c554" />
  <circle cx="22" cy="19" r="1.8" fill="#f7c554" />
  <circle cx="3.9" cy="26.4" r="2" fill="#0b1826" stroke="#22d3ee" stroke-width="1.5" />
  <circle cx="28.1" cy="26.4" r="2.3" fill="#f7c554" />
</svg>
