export interface FuzzyMatch {
  score: number;
  /** Posiciones (índices en `text`) de los caracteres que coinciden. */
  positions: number[];
}

function fold(s: string): string {
  return s.normalize('NFD').replace(/\p{M}/gu, '').toLowerCase();
}

/**
 * Búsqueda difusa por subsecuencia. Puntúa más alto: coincidencias seguidas,
 * al inicio de palabra y al principio del texto. `null` si no coincide.
 * Ignora mayúsculas y acentos (la longitud se conserva carácter a carácter).
 */
export function fuzzyMatch(query: string, text: string): FuzzyMatch | null {
  const q = fold(query).replace(/\s+/g, '');
  if (q === '') return { score: 0, positions: [] };
  const t = fold(text);
  if (t.length !== text.length) return fuzzyMatchSlow(q, text);
  return run(q, t);
}

function fuzzyMatchSlow(q: string, text: string): FuzzyMatch | null {
  return run(q, text.toLowerCase());
}

function run(q: string, t: string): FuzzyMatch | null {
  const positions: number[] = [];
  let score = 0;
  let ti = 0;
  let prev = -2;
  for (const qc of q) {
    const found = t.indexOf(qc, ti);
    if (found < 0) return null;
    positions.push(found);
    const boundary = found === 0 || /[\s\-_/.:]/.test(t[found - 1] ?? '');
    score += 1;
    if (found === prev + 1) score += 3;
    if (boundary) score += 4;
    if (found === 0) score += 2;
    score -= Math.min(found - ti, 6) * 0.1;
    prev = found;
    ti = found + 1;
  }
  score -= t.length * 0.01;
  return { score, positions };
}

export function fuzzyFilter<T>(
  query: string,
  items: readonly T[],
  textOf: (item: T) => string
): { item: T; match: FuzzyMatch }[] {
  const out: { item: T; match: FuzzyMatch }[] = [];
  for (const item of items) {
    const match = fuzzyMatch(query, textOf(item));
    if (match) out.push({ item, match });
  }
  return query.trim() === '' ? out : out.sort((a, b) => b.match.score - a.match.score);
}
