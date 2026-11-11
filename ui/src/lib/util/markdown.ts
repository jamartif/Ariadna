/**
 * Markdown seguro para las PR (descripciones y comentarios). Se convierte a un AST propio y la
 * interfaz lo pinta por nodos (`MarkdownView.svelte`): no hay `{@html}`, el HTML en bruto se
 * muestra como texto, las imágenes no se cargan y los enlaces solo pueden ser `https:`.
 *
 * Cubre lo habitual en PR: títulos, párrafos, listas (con tareas), citas, bloques de código,
 * `código`, **negrita**, *cursiva*, ~~tachado~~, enlaces y URL sueltas. Lo demás (tablas, HTML)
 * se queda como texto.
 */

export type Inline =
  | { t: 'text'; v: string }
  | { t: 'code'; v: string }
  | { t: 'strong'; c: Inline[] }
  | { t: 'em'; c: Inline[] }
  | { t: 'del'; c: Inline[] }
  | { t: 'link'; href: string; c: Inline[] }
  | { t: 'br' };

export interface ListItem {
  /** `true`/`false` = casilla marcada/sin marcar; `null` = elemento normal. */
  task: boolean | null;
  c: Block[];
}

export type Block =
  | { t: 'p'; c: Inline[] }
  | { t: 'h'; level: 1 | 2 | 3 | 4 | 5 | 6; c: Inline[] }
  | { t: 'code'; v: string; lang: string }
  | { t: 'quote'; c: Block[] }
  | { t: 'list'; ordered: boolean; start: number; items: ListItem[] }
  | { t: 'hr' };

/** Tope de entrada: lo que pase se ignora (el proveedor ya limita, pero la UI no debe colgarse). */
export const MARKDOWN_MAX_CHARS = 200_000;
const MAX_DEPTH = 6;

/** Solo `https:` y sin credenciales en la URL. Devuelve la URL normalizada o `null`. */
export function safeHttpsUrl(raw: string): string | null {
  const s = raw.trim();
  if (s === '' || s.length > 2048 || /[\u0000-\u001f\u007f\s]/.test(s)) return null;
  try {
    const u = new URL(s);
    if (u.protocol !== 'https:' || u.username !== '' || u.password !== '' || u.hostname === '') return null;
    return u.href;
  } catch {
    return null;
  }
}

const PUNCT = /[\\`*_{}[\]()#+\-.!~<>|"']/;

function isWordChar(c: string | undefined): boolean {
  return c !== undefined && /[\p{L}\p{N}]/u.test(c);
}

function pushText(out: Inline[], v: string): void {
  if (v === '') return;
  const last = out[out.length - 1];
  if (last && last.t === 'text') last.v += v;
  else out.push({ t: 'text', v });
}

function linkNode(href: string, c: Inline[]): Inline {
  return { t: 'link', href, c };
}

export function parseInline(src: string, depth = 0): Inline[] {
  const out: Inline[] = [];
  if (depth > 4) {
    pushText(out, src);
    return out;
  }
  let i = 0;
  const n = src.length;
  while (i < n) {
    const ch = src[i] as string;
    const rest = src.slice(i);

    if (ch === '\\' && i + 1 < n) {
      const nx = src[i + 1] as string;
      if (nx === '\n') {
        out.push({ t: 'br' });
        i += 2;
        continue;
      }
      if (PUNCT.test(nx)) {
        pushText(out, nx);
        i += 2;
        continue;
      }
    }

    if (ch === '\n') {
      // Salto «duro»: dos espacios al final de la línea anterior (ya recortados en `hard`).
      pushText(out, ' ');
      i += 1;
      continue;
    }

    if (ch === '`') {
      const run = /^`+/.exec(rest)?.[0] ?? '`';
      const close = src.indexOf(run, i + run.length);
      if (close > i + run.length - 1 && !(src[close + run.length] === '`')) {
        out.push({ t: 'code', v: src.slice(i + run.length, close).replace(/^ (.*) $/s, '$1') });
        i = close + run.length;
        continue;
      }
      pushText(out, run);
      i += run.length;
      continue;
    }

    if (ch === '!' && src[i + 1] === '[') {
      const m = /^!\[((?:[^\]\\]|\\.)*)\]\(\s*<?([^)\s>]+)>?(?:\s+"[^"]*")?\s*\)/.exec(rest);
      if (m) {
        // Las imágenes no se cargan (privacidad): se muestra el texto alternativo, enlazado si es https.
        const alt = (m[1] ?? '').replace(/\\(.)/g, '$1') || 'imagen';
        const href = safeHttpsUrl(m[2] as string);
        if (href) out.push(linkNode(href, [{ t: 'text', v: alt }]));
        else pushText(out, alt);
        i += m[0].length;
        continue;
      }
    }

    if (ch === '[') {
      const m = /^\[((?:[^\]\\]|\\.)*)\]\(\s*<?([^)\s>]+)>?(?:\s+"[^"]*")?\s*\)/.exec(rest);
      if (m) {
        const label = parseInline(m[1] as string, depth + 1);
        const href = safeHttpsUrl(m[2] as string);
        if (href) out.push(linkNode(href, label.length ? label : [{ t: 'text', v: href }]));
        else out.push(...label);
        i += m[0].length;
        continue;
      }
    }

    if (ch === '<') {
      const m = /^<(https:\/\/[^\s<>]+)>/.exec(rest);
      if (m) {
        const href = safeHttpsUrl(m[1] as string);
        if (href) {
          out.push(linkNode(href, [{ t: 'text', v: m[1] as string }]));
          i += m[0].length;
          continue;
        }
      }
    }

    if ((ch === '*' || ch === '_' || ch === '~') && src[i + 1] === ch) {
      const marker = ch + ch;
      const close = src.indexOf(marker, i + 2);
      const inner = close > i + 2 ? src.slice(i + 2, close) : '';
      const okOpen = ch !== '_' || !isWordChar(src[i - 1]);
      if (okOpen && inner !== '' && !/^\s/.test(inner) && !/\s$/.test(inner)) {
        const c = parseInline(inner, depth + 1);
        out.push(ch === '~' ? { t: 'del', c } : { t: 'strong', c });
        i = close + 2;
        continue;
      }
    }

    if (ch === '*' || ch === '_') {
      let close = -1;
      for (let j = i + 1; j < n; j++) {
        if (src[j] === '\\') {
          j++;
          continue;
        }
        if (src[j] === ch && src[j + 1] !== ch && src[j - 1] !== ch) {
          close = j;
          break;
        }
      }
      const inner = close > i + 1 ? src.slice(i + 1, close) : '';
      const okOpen = ch !== '_' || !isWordChar(src[i - 1]);
      const okClose = ch !== '_' || !isWordChar(src[close + 1]);
      if (okOpen && okClose && inner !== '' && !/^\s/.test(inner) && !/\s$/.test(inner)) {
        out.push({ t: 'em', c: parseInline(inner, depth + 1) });
        i = close + 1;
        continue;
      }
    }

    if (ch === 'h' && rest.startsWith('https://') && !isWordChar(src[i - 1])) {
      const m = /^https:\/\/[^\s<>]+/.exec(rest);
      if (m) {
        let url = m[0];
        // Sin la puntuación final de la frase (y sin un paréntesis que cierra uno ajeno).
        while (/[.,;:!?'"]$/.test(url) || (url.endsWith(')') && !url.includes('('))) url = url.slice(0, -1);
        const href = safeHttpsUrl(url);
        if (href) {
          out.push(linkNode(href, [{ t: 'text', v: url }]));
          i += url.length;
          continue;
        }
      }
    }

    pushText(out, ch);
    i += 1;
  }
  return out;
}

// ---- Bloques ---------------------------------------------------------------------------

const FENCE = /^(\s{0,3})(`{3,}|~{3,})\s*([^`\s]*)[^`]*$/;
const HEADING = /^\s{0,3}(#{1,6})\s+(.*?)(?:\s+#+)?\s*$/;
const HR = /^\s{0,3}([-*_])(?:\s*\1){2,}\s*$/;
const ITEM = /^(\s*)([-*+]|\d{1,9}[.)])\s+(.*)$/;
const QUOTE = /^\s{0,3}>\s?(.*)$/;

const indentOf = (l: string): number => {
  let n = 0;
  for (const c of l) {
    if (c === ' ') n++;
    else if (c === '\t') n += 4;
    else break;
  }
  return n;
};
const dedent = (l: string, by: number): string => {
  let removed = 0;
  let k = 0;
  while (k < l.length && removed < by) {
    if (l[k] === ' ') removed++;
    else if (l[k] === '\t') removed += 4;
    else break;
    k++;
  }
  return l.slice(k);
};

function paragraph(lines: string[]): Block {
  // Dos espacios al final de una línea = salto duro.
  const text = lines.map((l, idx) => (idx < lines.length - 1 && / {2,}$/.test(l) ? `${l.trim()}\\` : l.trim())).join('\n');
  return { t: 'p', c: parseInline(text) };
}

function parseBlocks(lines: string[], depth: number): Block[] {
  const out: Block[] = [];
  let i = 0;
  while (i < lines.length) {
    const line = lines[i] as string;
    if (line.trim() === '') {
      i++;
      continue;
    }

    const fence = FENCE.exec(line);
    if (fence) {
      const marker = fence[2] as string;
      const body: string[] = [];
      i++;
      while (i < lines.length && !new RegExp(`^\\s{0,3}${marker[0] === '`' ? '`' : '~'}{${marker.length},}\\s*$`).test(lines[i] as string)) {
        body.push(dedent(lines[i] as string, (fence[1] as string).length));
        i++;
      }
      i++; // cierre (o fin del texto)
      out.push({ t: 'code', v: body.join('\n'), lang: (fence[3] as string).slice(0, 32) });
      continue;
    }

    const h = HEADING.exec(line);
    if (h) {
      out.push({ t: 'h', level: (h[1] as string).length as 1 | 2 | 3 | 4 | 5 | 6, c: parseInline((h[2] as string).trim()) });
      i++;
      continue;
    }

    if (HR.test(line)) {
      out.push({ t: 'hr' });
      i++;
      continue;
    }

    if (QUOTE.test(line)) {
      const inner: string[] = [];
      while (i < lines.length && QUOTE.test(lines[i] as string)) {
        inner.push((QUOTE.exec(lines[i] as string) as RegExpExecArray)[1] as string);
        i++;
      }
      out.push({ t: 'quote', c: depth >= MAX_DEPTH ? [paragraph(inner)] : parseBlocks(inner, depth + 1) });
      continue;
    }

    const item = ITEM.exec(line);
    if (item && depth < MAX_DEPTH) {
      const base = indentOf(line);
      const ordered = /\d/.test(item[2] as string);
      const start = ordered ? parseInt(item[2] as string, 10) : 1;
      const items: ListItem[] = [];
      while (i < lines.length) {
        const cur = lines[i] as string;
        const m = ITEM.exec(cur);
        if (!m || indentOf(cur) !== base || /\d/.test(m[2] as string) !== ordered) break;
        let first = m[3] as string;
        let task: boolean | null = null;
        const tk = /^\[([ xX])\]\s+(.*)$/.exec(first);
        if (tk) {
          task = tk[1] !== ' ';
          first = tk[2] as string;
        }
        const content: string[] = [first];
        const contentIndent = base + (m[2] as string).length + 1;
        i++;
        while (i < lines.length) {
          const nx = lines[i] as string;
          if (nx.trim() === '') {
            // Línea en blanco: sigue el elemento solo si lo siguiente está sangrado.
            let k = i;
            while (k < lines.length && (lines[k] as string).trim() === '') k++;
            if (k < lines.length && indentOf(lines[k] as string) > base) {
              for (; i < k; i++) content.push('');
              continue;
            }
            break;
          }
          if (indentOf(nx) > base) {
            content.push(dedent(nx, Math.min(contentIndent, indentOf(nx))));
            i++;
            continue;
          }
          // Continuación perezosa: texto pegado sin marcador de lista.
          if (!ITEM.test(nx) && !FENCE.test(nx) && !HEADING.test(nx) && !QUOTE.test(nx) && !HR.test(nx) && (content[content.length - 1] ?? '') !== '') {
            content.push(nx.trim());
            i++;
            continue;
          }
          break;
        }
        items.push({ task, c: parseBlocks(content, depth + 1) });
        // Entre elementos puede haber líneas en blanco.
        let k = i;
        while (k < lines.length && (lines[k] as string).trim() === '') k++;
        const after = lines[k];
        const am = after === undefined ? null : ITEM.exec(after);
        if (am && indentOf(after as string) === base && /\d/.test(am[2] as string) === ordered) i = k;
      }
      out.push({ t: 'list', ordered, start, items });
      continue;
    }

    // Párrafo: hasta una línea en blanco o el inicio de otro bloque.
    const para: string[] = [];
    while (i < lines.length) {
      const l = lines[i] as string;
      if (l.trim() === '' || (para.length > 0 && (FENCE.test(l) || HEADING.test(l) || HR.test(l) || QUOTE.test(l) || ITEM.test(l)))) break;
      para.push(l);
      i++;
    }
    out.push(paragraph(para));
  }
  return out;
}

/** Quita los comentarios HTML (`<!-- … -->`, habituales en las plantillas): no se ven en Markdown. */
function stripComments(s: string): string {
  return s.replace(/<!--[\s\S]*?(?:-->|$)/g, '');
}

export function parseMarkdown(src: string): Block[] {
  const text = stripComments(src.length > MARKDOWN_MAX_CHARS ? src.slice(0, MARKDOWN_MAX_CHARS) : src).replace(/\r\n?/g, '\n');
  return parseBlocks(text.split('\n'), 0);
}

/** Texto plano de un fragmento (para el título de un enlace, pruebas…). */
export function inlineText(nodes: Inline[]): string {
  return nodes
    .map((n) => {
      switch (n.t) {
        case 'text':
        case 'code':
          return n.v;
        case 'br':
          return ' ';
        default:
          return inlineText(n.c);
      }
    })
    .join('');
}
