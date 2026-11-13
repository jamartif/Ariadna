/**
 * Coloreado de sintaxis para el diff. Tokeniza con Prism (sin `{@html}`): el
 * resultado son segmentos `{ text, kind }` que Svelte pinta como `<span>`. Cada
 * gramática se carga bajo demanda (un chunk por lenguaje) para no inflar el bundle.
 */
import type { Range } from './intraline';

export type Lang =
  | 'rust'
  | 'typescript'
  | 'javascript'
  | 'json'
  | 'yaml'
  | 'toml'
  | 'markdown'
  | 'css'
  | 'markup'
  | 'python'
  | 'csharp'
  | 'bash'
  | 'sql'
  | 'text';

export type TokenKind =
  | 'comment'
  | 'string'
  | 'keyword'
  | 'function'
  | 'number'
  | 'type'
  | 'property'
  | 'operator'
  | 'punct'
  | 'variable'
  | 'tag'
  | 'regex'
  | 'heading';

export interface Segment {
  text: string;
  kind: TokenKind | null;
  /** Dentro del rango cambiado de la línea (resaltado intra-línea). */
  changed: boolean;
}

const EXT: Record<string, Lang> = {
  rs: 'rust',
  ts: 'typescript',
  tsx: 'typescript',
  mts: 'typescript',
  cts: 'typescript',
  js: 'javascript',
  jsx: 'javascript',
  mjs: 'javascript',
  cjs: 'javascript',
  json: 'json',
  jsonc: 'json',
  yml: 'yaml',
  yaml: 'yaml',
  toml: 'toml',
  md: 'markdown',
  markdown: 'markdown',
  css: 'css',
  scss: 'css',
  html: 'markup',
  htm: 'markup',
  xml: 'markup',
  svg: 'markup',
  svelte: 'markup',
  vue: 'markup',
  csproj: 'markup',
  py: 'python',
  cs: 'csharp',
  sh: 'bash',
  bash: 'bash',
  zsh: 'bash',
  sql: 'sql'
};

const BY_NAME: Record<string, Lang> = {
  makefile: 'bash',
  dockerfile: 'bash',
  '.gitignore': 'bash',
  '.bashrc': 'bash',
  'cargo.lock': 'toml'
};

/** Lenguaje por nombre de archivo (extensión o nombre conocido). */
export function languageOf(path: string): Lang {
  const base = (path.split('/').pop() ?? path).toLowerCase();
  const named = BY_NAME[base];
  if (named) return named;
  const dot = base.lastIndexOf('.');
  if (dot < 0) return 'text';
  return EXT[base.slice(dot + 1)] ?? 'text';
}

// ---- Carga perezosa -----------------------------------------------------------------

type PrismCore = typeof import('prismjs');

const LOADERS: Record<Exclude<Lang, 'text'> | 'clike', () => Promise<unknown>> = {
  clike: () => import('prismjs/components/prism-clike.js'),
  markup: () => import('prismjs/components/prism-markup.js'),
  css: () => import('prismjs/components/prism-css.js'),
  javascript: () => import('prismjs/components/prism-javascript.js'),
  typescript: () => import('prismjs/components/prism-typescript.js'),
  json: () => import('prismjs/components/prism-json.js'),
  yaml: () => import('prismjs/components/prism-yaml.js'),
  toml: () => import('prismjs/components/prism-toml.js'),
  markdown: () => import('prismjs/components/prism-markdown.js'),
  python: () => import('prismjs/components/prism-python.js'),
  csharp: () => import('prismjs/components/prism-csharp.js'),
  bash: () => import('prismjs/components/prism-bash.js'),
  sql: () => import('prismjs/components/prism-sql.js'),
  rust: () => import('prismjs/components/prism-rust.js')
};

const DEPS: Partial<Record<keyof typeof LOADERS, (keyof typeof LOADERS)[]>> = {
  javascript: ['clike'],
  typescript: ['clike', 'javascript'],
  csharp: ['clike'],
  markdown: ['markup']
};

let core: PrismCore | null = null;
let corePromise: Promise<PrismCore> | null = null;
const loaded = new Map<string, Promise<void>>();
const ready = new Set<string>();

async function loadCore(): Promise<PrismCore> {
  if (core) return core;
  corePromise ??= import('prismjs').then((m) => {
    const p = ((m as { default?: PrismCore }).default ?? m) as PrismCore;
    // Prism pinta solo (autorun) y sus componentes buscan el global `Prism`.
    p.manual = true;
    (globalThis as { Prism?: PrismCore }).Prism = p;
    core = p;
    return p;
  });
  return corePromise;
}

function loadOne(name: keyof typeof LOADERS): Promise<void> {
  let p = loaded.get(name);
  if (!p) {
    p = (async () => {
      for (const d of DEPS[name] ?? []) await loadOne(d);
      await loadCore();
      await LOADERS[name]();
      ready.add(name);
    })().catch((e) => {
      loaded.delete(name);
      throw e;
    });
    loaded.set(name, p);
  }
  return p;
}

/** Carga la gramática de un lenguaje. `text` no necesita nada. */
export async function loadLanguage(lang: Lang): Promise<void> {
  if (lang === 'text') return;
  await loadOne(lang);
}

export function isLanguageReady(lang: Lang): boolean {
  return lang === 'text' || ready.has(lang);
}

// ---- Tokenización ---------------------------------------------------------------------

const KIND: Record<string, TokenKind> = {
  comment: 'comment',
  prolog: 'comment',
  doctype: 'comment',
  cdata: 'comment',
  string: 'string',
  char: 'string',
  'attr-value': 'string',
  'template-string': 'string',
  'triple-quoted-string': 'string',
  'interpolation-string': 'string',
  url: 'string',
  keyword: 'keyword',
  atrule: 'keyword',
  important: 'keyword',
  directive: 'keyword',
  'control-flow': 'keyword',
  function: 'function',
  'function-variable': 'function',
  macro: 'function',
  number: 'number',
  boolean: 'number',
  constant: 'number',
  'class-name': 'type',
  builtin: 'type',
  namespace: 'type',
  'maybe-class-name': 'type',
  property: 'property',
  'attr-name': 'property',
  key: 'property',
  selector: 'property',
  tag: 'tag',
  operator: 'operator',
  punctuation: 'punct',
  variable: 'variable',
  symbol: 'variable',
  regex: 'regex',
  title: 'heading',
  bold: 'heading',
  inserted: 'string',
  deleted: 'keyword'
};

function kindOf(type: string, alias?: string | string[]): TokenKind | null {
  const names = [type, ...(Array.isArray(alias) ? alias : alias ? [alias] : [])];
  for (const n of names) {
    const k = KIND[n];
    if (k) return k;
  }
  return null;
}

interface Flat {
  text: string;
  kind: TokenKind | null;
}

function flatten(stream: (string | { type: string; alias?: string | string[]; content: unknown })[], inherited: TokenKind | null, out: Flat[]): void {
  for (const t of stream) {
    if (typeof t === 'string') {
      if (t !== '') out.push({ text: t, kind: inherited });
      continue;
    }
    const own = kindOf(t.type, t.alias) ?? inherited;
    const c = t.content;
    if (typeof c === 'string') out.push({ text: c, kind: own });
    else if (Array.isArray(c)) flatten(c as never, own, out);
  }
}

const cache = new Map<string, Flat[]>();
const CACHE_MAX = 6000;

/**
 * Tokeniza una línea. Si la gramática aún no está cargada (o el lenguaje es texto)
 * devuelve la línea sin colorear; el llamador relanza el render al terminar la carga.
 */
export function tokenizeLine(lang: Lang, text: string): Flat[] {
  if (lang === 'text' || !core || !ready.has(lang) || text === '') return [{ text, kind: null }];
  const key = `${lang}\u0000${text}`;
  const hit = cache.get(key);
  if (hit) return hit;
  const grammar = core.languages[lang];
  let out: Flat[];
  if (!grammar) out = [{ text, kind: null }];
  else {
    out = [];
    try {
      flatten(core.tokenize(text, grammar) as never, null, out);
    } catch {
      out = [{ text, kind: null }];
    }
    if (out.length === 0) out = [{ text, kind: null }];
  }
  if (cache.size >= CACHE_MAX) cache.clear();
  cache.set(key, out);
  return out;
}

/**
 * Une los tokens de sintaxis con los rangos cambiados de la línea (intra-línea): corta
 * los tokens en las fronteras de los rangos y marca `changed` donde corresponda.
 */
export function segmentLine(lang: Lang, text: string, ranges: readonly Range[] = []): Segment[] {
  const tokens = tokenizeLine(lang, text);
  if (ranges.length === 0) return tokens.map((t) => ({ ...t, changed: false }));
  const out: Segment[] = [];
  let pos = 0;
  for (const tok of tokens) {
    let from = 0;
    const end = pos + tok.text.length;
    const bounds = new Set<number>();
    for (const r of ranges) {
      if (r.start > pos && r.start < end) bounds.add(r.start - pos);
      if (r.end > pos && r.end < end) bounds.add(r.end - pos);
    }
    for (const cut of [...bounds].sort((a, b) => a - b).concat(tok.text.length)) {
      if (cut <= from) continue;
      const a = pos + from;
      out.push({
        text: tok.text.slice(from, cut),
        kind: tok.kind,
        changed: ranges.some((r) => a >= r.start && a < r.end)
      });
      from = cut;
    }
    pos = end;
  }
  return out;
}
