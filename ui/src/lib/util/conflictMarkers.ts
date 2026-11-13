/**
 * Marcadores de conflicto (`<<<<<<<`, `|||||||`, `=======`, `>>>>>>>`) sobre texto.
 * Espejo de `crates/ariadna-git/src/conflict.rs`: marcador = exactamente 7 caracteres,
 * solos en la línea o seguidos de un espacio y una etiqueta. Conserva `\r\n` y cualquier
 * otro byte fuera de las regiones (opera por offsets sobre la cadena original).
 */

export type HunkChoiceSide = 'ours' | 'theirs' | 'base' | 'both';

export interface MarkerRegion {
  id: number;
  /** Offset del inicio de la línea `<<<<<<<` (tras un BOM inicial, si lo hay). */
  start: number;
  /** Offset final (exclusivo) de la línea `>>>>>>>`, con su salto de línea. */
  end: number;
  /** Línea (0-based) del `<<<<<<<` en el texto. */
  line: number;
  ours: string;
  base: string | null;
  theirs: string;
}

export interface ParsedMarkers {
  regions: MarkerRegion[];
  /** Hay un `<<<<<<<` sin su `>>>>>>>`. */
  unterminated: boolean;
}

const BOM = '\uFEFF';

function isMarker(line: string, c: string): boolean {
  return line.length >= 7 && line.slice(0, 7) === c.repeat(7) && (line.length === 7 || line[7] === ' ');
}

function stripEol(line: string): string {
  const l = line.endsWith('\n') ? line.slice(0, -1) : line;
  return l.endsWith('\r') ? l.slice(0, -1) : l;
}

interface Ln {
  raw: string;
  start: number;
  end: number;
}

function lines(text: string): Ln[] {
  const out: Ln[] = [];
  let start = 0;
  while (start < text.length) {
    const nl = text.indexOf('\n', start);
    const end = nl < 0 ? text.length : nl + 1;
    out.push({ raw: text.slice(start, end), start, end });
    start = end;
  }
  return out;
}

export function parseMarkers(text: string): ParsedMarkers {
  const ls = lines(text);
  const regions: MarkerRegion[] = [];
  let unterminated = false;
  let i = 0;
  while (i < ls.length) {
    const first = ls[i] as Ln;
    const bom = i === 0 && first.raw.startsWith(BOM) ? 1 : 0;
    if (!isMarker(stripEol(first.raw.slice(bom)), '<')) {
      i++;
      continue;
    }
    const start = first.start + bom;
    const line = i;
    let ours = '';
    let base = '';
    let theirs = '';
    let sawBase = false;
    let phase = 1;
    let closed = false;
    i++;
    while (i < ls.length) {
      const cur = ls[i] as Ln;
      const text1 = stripEol(cur.raw);
      if (phase === 1 && isMarker(text1, '|')) {
        sawBase = true;
        phase = 2;
      } else if ((phase === 1 || phase === 2) && isMarker(text1, '=')) {
        phase = 3;
      } else if (phase === 3 && isMarker(text1, '>')) {
        regions.push({ id: regions.length, start, end: cur.end, line, ours, base: sawBase ? base : null, theirs });
        i++;
        closed = true;
        break;
      } else if (phase === 1) ours += cur.raw;
      else if (phase === 2) base += cur.raw;
      else theirs += cur.raw;
      i++;
    }
    if (!closed) {
      unterminated = true;
      break;
    }
  }
  return { regions, unterminated };
}

/** ¿Quedan marcadores (bloques o un `<<<<<<<` sin cerrar)? */
export function hasMarkers(text: string): boolean {
  const p = parseMarkers(text);
  return p.regions.length > 0 || p.unterminated;
}

function pick(r: MarkerRegion, side: HunkChoiceSide): string | null {
  switch (side) {
    case 'ours':
      return r.ours;
    case 'theirs':
      return r.theirs;
    case 'both':
      return r.ours + r.theirs;
    case 'base':
      return r.base;
  }
}

/** Resuelve un bloque (por índice). `null` si el bloque no existe o no tiene base. */
export function resolveRegion(text: string, id: number, side: HunkChoiceSide): string | null {
  const p = parseMarkers(text);
  if (p.unterminated) return null;
  const r = p.regions.find((x) => x.id === id);
  if (!r) return null;
  const rep = pick(r, side);
  if (rep === null) return null;
  return text.slice(0, r.start) + rep + text.slice(r.end);
}

/** Resuelve todos los bloques con el mismo lado. Los que no tengan base se dejan como están. */
export function resolveAllRegions(text: string, side: HunkChoiceSide): string {
  const p = parseMarkers(text);
  if (p.unterminated) return text;
  let out = '';
  let cursor = 0;
  for (const r of p.regions) {
    out += text.slice(cursor, r.start);
    out += pick(r, side) ?? text.slice(r.start, r.end);
    cursor = r.end;
  }
  return out + text.slice(cursor);
}

/**
 * Líneas (0-based, medio abierto) donde aparece `snippet` dentro de `content` buscando a
 * partir de la línea `from`. Sirve para resaltar en los paneles ours/base/theirs el bloque
 * que se está resolviendo. `null` si no se encuentra o el fragmento está vacío.
 */
export function locateLines(content: string, snippet: string, from = 0): { start: number; end: number } | null {
  if (snippet === '') return null;
  const ls = lines(content);
  const offset = ls[from]?.start ?? content.length;
  const at = content.indexOf(snippet, offset);
  if (at < 0) return null;
  let start = 0;
  for (let k = 0; k < ls.length; k++) {
    if ((ls[k] as Ln).end > at) {
      start = k;
      break;
    }
  }
  const count = Math.max(1, lines(snippet).length);
  return { start, end: start + count };
}

/** Final de línea dominante (CRLF solo si hay al menos tantos CRLF como LF sueltos). */
export function dominantEol(text: string): '\r\n' | '\n' {
  const crlf = (text.match(/\r\n/g) ?? []).length;
  const lf = (text.match(/\n/g) ?? []).length - crlf;
  return crlf > 0 && crlf >= lf ? '\r\n' : '\n';
}

/** Normaliza todos los saltos de línea de `text` al final indicado. */
export function withEol(text: string, eol: '\r\n' | '\n'): string {
  const lf = text.replace(/\r\n/g, '\n');
  return eol === '\n' ? lf : lf.replace(/\n/g, '\r\n');
}

/** Líneas de un texto (sin el salto final), para pintar con números. */
export function splitLines(text: string): string[] {
  if (text === '') return [];
  const out = text.split('\n').map((l) => (l.endsWith('\r') ? l.slice(0, -1) : l));
  if (text.endsWith('\n')) out.pop();
  return out;
}
