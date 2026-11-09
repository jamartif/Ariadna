/** Utilidades de ventana para listas virtualizadas con alturas variables. */

export function buildOffsets(count: number, heightOf: (i: number) => number): Float64Array {
  const offsets = new Float64Array(count + 1);
  for (let i = 0; i < count; i++) offsets[i + 1] = (offsets[i] ?? 0) + heightOf(i);
  return offsets;
}

/** Índice del elemento que contiene la posición `y` (búsqueda binaria). */
export function indexAt(offsets: Float64Array, y: number): number {
  const count = offsets.length - 1;
  if (count <= 0) return 0;
  let lo = 0;
  let hi = count - 1;
  while (lo < hi) {
    const mid = (lo + hi + 1) >> 1;
    if ((offsets[mid] ?? 0) <= y) lo = mid;
    else hi = mid - 1;
  }
  return lo;
}

export function visibleRange(
  offsets: Float64Array,
  scrollTop: number,
  viewport: number,
  overscan: number
): { start: number; end: number } {
  const count = offsets.length - 1;
  if (count <= 0) return { start: 0, end: 0 };
  const start = Math.max(0, indexAt(offsets, scrollTop) - overscan);
  const last = indexAt(offsets, scrollTop + viewport);
  const end = Math.min(count, last + 1 + overscan);
  return { start, end };
}

/** Reordena `ids` moviendo `id` `delta` posiciones (acotado). Devuelve el mismo array si no cambia. */
export function moveItem<T>(items: readonly T[], from: number, delta: number): T[] {
  const to = Math.max(0, Math.min(items.length - 1, from + delta));
  if (from < 0 || from >= items.length || to === from) return [...items];
  const next = [...items];
  const [it] = next.splice(from, 1);
  next.splice(to, 0, it as T);
  return next;
}
