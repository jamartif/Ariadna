/**
 * Transiciones de pestañas. Duraciones de 120–180 ms; se anulan con
 * `prefers-reduced-motion`. Devuelven `css` (Svelte las ejecuta con WAAPI).
 */
export function reducedMotion(): boolean {
  return typeof window !== 'undefined' && typeof window.matchMedia === 'function' && window.matchMedia('(prefers-reduced-motion: reduce)').matches;
}

const easeOut = (t: number) => 1 - (1 - t) ** 3;

export function tabIn(_node: Element) {
  const duration = reducedMotion() ? 0 : 180;
  return {
    duration,
    css: (t: number) => {
      const e = easeOut(t);
      return `opacity:${e};transform:translateY(${(1 - e) * 5}px) scale(${0.94 + 0.06 * e});`;
    }
  };
}

export function tabOut(_node: Element) {
  const duration = reducedMotion() ? 0 : 120;
  return {
    duration,
    css: (t: number) => `opacity:${t};transform:scale(${0.92 + 0.08 * t});`
  };
}
