/**
 * Diferencias línea a línea para mostrar una propuesta de la IA contra lo que hay ahora.
 *
 * Es solo presentación: no toca git ni produce parches. Recorta el contexto (las líneas
 * iguales lejos de cualquier cambio se resumen en una línea `gap`) para que el panel quepa.
 */

export type DiffKind = 'same' | 'add' | 'del' | 'gap';

export interface DiffRow {
  kind: DiffKind;
  text: string;
}

/** Tope de líneas del tramo que se compara con LCS (por encima, todo el tramo es cambio). */
const MAX_CELLS = 4_000_000;

function lcsRows(a: string[], b: string[]): DiffRow[] {
  const n = a.length;
  const m = b.length;
  const dp: number[][] = Array.from({ length: n + 1 }, () => new Array<number>(m + 1).fill(0));
  for (let i = n - 1; i >= 0; i--) {
    const row = dp[i] as number[];
    const next = dp[i + 1] as number[];
    for (let j = m - 1; j >= 0; j--) {
      row[j] = a[i] === b[j] ? (next[j + 1] as number) + 1 : Math.max(next[j] as number, row[j + 1] as number);
    }
  }
  const out: DiffRow[] = [];
  let i = 0;
  let j = 0;
  while (i < n && j < m) {
    if (a[i] === b[j]) {
      out.push({ kind: 'same', text: a[i] as string });
      i++;
      j++;
    } else if ((dp[i + 1] as number[])[j]! >= (dp[i] as number[])[j + 1]!) {
      out.push({ kind: 'del', text: a[i] as string });
      i++;
    } else {
      out.push({ kind: 'add', text: b[j] as string });
      j++;
    }
  }
  while (i < n) out.push({ kind: 'del', text: a[i++] as string });
  while (j < m) out.push({ kind: 'add', text: b[j++] as string });
  return out;
}

/** Deja `context` líneas alrededor de cada cambio y resume cada hueco en una fila `gap`. */
function clip(rows: DiffRow[], context: number): DiffRow[] {
  const changed: number[] = [];
  rows.forEach((r, i) => {
    if (r.kind !== 'same') changed.push(i);
  });
  if (changed.length === 0) return rows;
  const keep = new Uint8Array(rows.length);
  for (const i of changed) {
    const from = Math.max(0, i - context);
    const to = Math.min(rows.length - 1, i + context);
    for (let k = from; k <= to; k++) keep[k] = 1;
  }
  const out: DiffRow[] = [];
  let inGap = false;
  rows.forEach((r, i) => {
    if (keep[i] === 1) {
      out.push(r);
      inGap = false;
    } else if (!inGap && out.length > 0) {
      out.push({ kind: 'gap', text: '' });
      inGap = true;
    }
  });
  // Un hueco al final no aporta nada: el panel ya se corta ahí.
  if (out[out.length - 1]?.kind === 'gap') out.pop();
  return out;
}

/**
 * Filas de la diferencia entre `before` (lo que hay ahora en el editor) y `after` (la
 * propuesta). Los textos se comparan por líneas y sin normalizar (se llaman ya en LF).
 */
export function lineDiff(before: string, after: string, context = 2): DiffRow[] {
  const a = before.split('\n');
  const b = after.split('\n');

  let start = 0;
  while (start < a.length && start < b.length && a[start] === b[start]) start++;
  let endA = a.length;
  let endB = b.length;
  while (endA > start && endB > start && a[endA - 1] === b[endB - 1]) {
    endA--;
    endB--;
  }

  const midA = a.slice(start, endA);
  const midB = b.slice(start, endB);
  let mid: DiffRow[];
  if (midA.length === 0) {
    mid = midB.map((text) => ({ kind: 'add' as const, text }));
  } else if (midB.length === 0) {
    mid = midA.map((text) => ({ kind: 'del' as const, text }));
  } else if (midA.length * midB.length > MAX_CELLS) {
    // Demasiado grande para LCS: se muestra el tramo entero como reemplazo.
    mid = [
      ...midA.map((text) => ({ kind: 'del' as const, text })),
      ...midB.map((text) => ({ kind: 'add' as const, text }))
    ];
  } else {
    mid = lcsRows(midA, midB);
  }

  const rows: DiffRow[] = [
    ...a.slice(0, start).map((text) => ({ kind: 'same' as const, text })),
    ...mid,
    ...a.slice(endA).map((text) => ({ kind: 'same' as const, text }))
  ];
  return clip(rows, context);
}

/** Cuántas líneas añade y cuántas quita la propuesta. */
export function diffStats(rows: DiffRow[]): { additions: number; deletions: number } {
  let additions = 0;
  let deletions = 0;
  for (const r of rows) {
    if (r.kind === 'add') additions++;
    else if (r.kind === 'del') deletions++;
  }
  return { additions, deletions };
}
