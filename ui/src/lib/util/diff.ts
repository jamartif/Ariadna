import type { DiffHunk, DiffLine, FileDiff } from '$lib/api/types';
import { pairLines } from './intraline';

/** Umbrales de NF-5 (spec §9). */
export const GIANT_LINES = 5000;
export const GIANT_BYTES = 1_000_000;
/** Longitud a partir de la cual una línea se trunca con «ver más». */
export const LONG_LINE = 500;

export function countLines(d: FileDiff): number {
  let n = 0;
  for (const h of d.hunks) n += h.lines.length;
  return n;
}

export function countBytes(d: FileDiff): number {
  let n = 0;
  for (const h of d.hunks) for (const l of h.lines) n += l.text.length + 1;
  return n;
}

export function isGiant(d: FileDiff): boolean {
  return countLines(d) > GIANT_LINES || countBytes(d) > GIANT_BYTES;
}

export function diffStats(d: FileDiff): { additions: number; deletions: number } {
  let additions = 0;
  let deletions = 0;
  for (const h of d.hunks)
    for (const l of h.lines) {
      if (l.kind === 'addition') additions++;
      else if (l.kind === 'deletion') deletions++;
    }
  return { additions, deletions };
}

export type DiffRow =
  | { type: 'hunk'; hunkIndex: number; hunk: DiffHunk }
  | { type: 'line'; hunkIndex: number; line: DiffLine; /** Línea emparejada (borrada ↔ añadida) para el resaltado intra-línea. */ partner: DiffLine | null };

/** Aplana hunks en filas para la lista virtualizada. */
export function flattenDiff(d: FileDiff): DiffRow[] {
  const rows: DiffRow[] = [];
  d.hunks.forEach((hunk, hunkIndex) => {
    rows.push({ type: 'hunk', hunkIndex, hunk });
    const pairs = pairLines(hunk.lines);
    for (const line of hunk.lines) rows.push({ type: 'line', hunkIndex, line, partner: pairs.get(line.id) ?? null });
  });
  return rows;
}

export function hunkHeader(h: DiffHunk): string {
  return `@@ -${h.oldStart},${h.oldCount} +${h.newStart},${h.newCount} @@`;
}

export function truncateLine(text: string, expanded: boolean): { text: string; truncated: boolean } {
  if (expanded || text.length <= LONG_LINE) return { text, truncated: false };
  return { text: text.slice(0, LONG_LINE), truncated: true };
}

/** Acorta el contexto de función de un hunk (`heading`) para la cabecera. */
export function hunkContext(h: DiffHunk): string {
  return h.heading.trim();
}
