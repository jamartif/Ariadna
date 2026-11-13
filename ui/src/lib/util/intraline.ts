import type { DiffLine } from '$lib/api/types';

export interface Range {
  start: number;
  end: number;
}

/** Palabras, espacios y signos sueltos: la unidad del resaltado dentro de la línea. */
export function splitWords(s: string): string[] {
  return s.match(/[\p{L}\p{N}_]+|\s+|[^\s\p{L}\p{N}_]/gu) ?? [];
}

const MAX_TOKENS = 220;
/** Por encima de esta fracción cambiada la línea es «otra» y no se resalta por dentro. */
const MAX_CHANGED_RATIO = 0.7;

function merge(ranges: Range[]): Range[] {
  const out: Range[] = [];
  for (const r of ranges) {
    const last = out[out.length - 1];
    if (last && r.start <= last.end) last.end = Math.max(last.end, r.end);
    else out.push({ ...r });
  }
  return out;
}

/**
 * Diferencias a nivel de palabra entre dos líneas (LCS sobre tokens). Devuelve los
 * rangos de caracteres que cambian en cada lado, o `null` si no merece la pena
 * resaltar (líneas casi distintas, vacías, o demasiado largas).
 */
export function wordDiff(a: string, b: string): { a: Range[]; b: Range[] } | null {
  if (a === b || a.trim() === '' || b.trim() === '') return null;
  const ta = splitWords(a);
  const tb = splitWords(b);
  if (ta.length > MAX_TOKENS || tb.length > MAX_TOKENS) return null;

  // Prefijo y sufijo comunes (lo habitual) antes de la programación dinámica.
  let pre = 0;
  while (pre < ta.length && pre < tb.length && ta[pre] === tb[pre]) pre++;
  let suf = 0;
  while (suf < ta.length - pre && suf < tb.length - pre && ta[ta.length - 1 - suf] === tb[tb.length - 1 - suf]) suf++;
  const ma = ta.slice(pre, ta.length - suf);
  const mb = tb.slice(pre, tb.length - suf);

  const n = ma.length;
  const m = mb.length;
  const keepA = new Array<boolean>(n).fill(false);
  const keepB = new Array<boolean>(m).fill(false);
  if (n > 0 && m > 0) {
    const w = m + 1;
    const dp = new Uint16Array((n + 1) * w);
    for (let i = n - 1; i >= 0; i--) {
      for (let j = m - 1; j >= 0; j--) {
        dp[i * w + j] = ma[i] === mb[j] ? (dp[(i + 1) * w + j + 1] as number) + 1 : Math.max(dp[(i + 1) * w + j] as number, dp[i * w + j + 1] as number);
      }
    }
    let i = 0;
    let j = 0;
    while (i < n && j < m) {
      if (ma[i] === mb[j]) {
        keepA[i] = true;
        keepB[j] = true;
        i++;
        j++;
      } else if ((dp[(i + 1) * w + j] as number) >= (dp[i * w + j + 1] as number)) i++;
      else j++;
    }
  }

  const toRanges = (tokens: string[], prefix: number, middle: string[], keep: boolean[]): Range[] => {
    let pos = 0;
    for (let k = 0; k < prefix; k++) pos += (tokens[k] as string).length;
    const out: Range[] = [];
    middle.forEach((tok, k) => {
      if (!keep[k]) out.push({ start: pos, end: pos + tok.length });
      pos += tok.length;
    });
    return merge(out);
  };
  const ra = toRanges(ta, pre, ma, keepA);
  const rb = toRanges(tb, pre, mb, keepB);
  const sum = (rs: Range[]) => rs.reduce((x, r) => x + (r.end - r.start), 0);
  if (sum(ra) / Math.max(1, a.length) > MAX_CHANGED_RATIO && sum(rb) / Math.max(1, b.length) > MAX_CHANGED_RATIO) return null;
  if (ra.length === 0 && rb.length === 0) return null;
  return { a: ra, b: rb };
}

/**
 * Empareja líneas borradas con añadidas: en cada bloque de borrados seguido de un
 * bloque de añadidos, la i-ésima con la i-ésima. Devuelve id de línea → línea pareja.
 */
export function pairLines(lines: readonly DiffLine[]): Map<string, DiffLine> {
  const out = new Map<string, DiffLine>();
  let i = 0;
  while (i < lines.length) {
    if ((lines[i] as DiffLine).kind !== 'deletion') {
      i++;
      continue;
    }
    const dels: DiffLine[] = [];
    while (i < lines.length && (lines[i] as DiffLine).kind === 'deletion') dels.push(lines[i++] as DiffLine);
    const adds: DiffLine[] = [];
    while (i < lines.length && (lines[i] as DiffLine).kind === 'addition') adds.push(lines[i++] as DiffLine);
    const n = Math.min(dels.length, adds.length);
    for (let k = 0; k < n; k++) {
      out.set((dels[k] as DiffLine).id, adds[k] as DiffLine);
      out.set((adds[k] as DiffLine).id, dels[k] as DiffLine);
    }
  }
  return out;
}

/** Rangos cambiados de `line` respecto a su pareja (si la hay). */
export function changedRanges(line: DiffLine, partner: DiffLine | null | undefined): Range[] {
  if (!partner) return [];
  const d = line.kind === 'deletion' ? wordDiff(line.text, partner.text) : wordDiff(partner.text, line.text);
  if (!d) return [];
  return line.kind === 'deletion' ? d.a : d.b;
}
