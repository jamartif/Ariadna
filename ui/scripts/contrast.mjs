#!/usr/bin/env node
/*
 * Contraste real de la paleta y de los componentes con degradado (sin dependencias).
 *
 * Uso:
 *   node scripts/contrast.mjs            # informe legible
 *   node scripts/contrast.mjs --md       # tablas Markdown (para specs/rust/contraste.md)
 *   node scripts/contrast.mjs --fail     # sale con código 1 si algo queda por debajo de AA
 *
 * Mide con la fórmula WCAG 2.2 (luminancia relativa) los pares que no se pueden leer con
 * `background-color` en el navegador: el botón primario (degradado y color plano) y las
 * marcas de proveedor. Además revisa el texto sobre superficie elevada.
 */

import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

const here = dirname(fileURLToPath(import.meta.url));
const root = join(here, '..');
const css = readFileSync(join(root, 'src/lib/styles/tokens.css'), 'utf8');
const providerSrc = readFileSync(join(root, 'src/lib/components/ProviderMark.svelte'), 'utf8');

/** Cuerpo del bloque cuyo selector empieza por `selector` (a partir de `from`). */
function block(selector, from = 0) {
  const start = css.indexOf(selector, from);
  if (start < 0) throw new Error(`No se encontró ${selector}`);
  const open = css.indexOf('{', start);
  let depth = 0;
  for (let i = open; i < css.length; i++) {
    if (css[i] === '{') depth++;
    else if (css[i] === '}' && --depth === 0) return css.slice(open + 1, i);
  }
  throw new Error(`Bloque sin cerrar: ${selector}`);
}

function vars(body) {
  const out = {};
  for (const m of body.matchAll(/--([\w-]+):\s*([^;]+);/g)) out[m[1]] = m[2].trim();
  return out;
}

const light = vars(block(':root {'));
const dark = vars(block(":root[data-theme='dark']"));

/* ---- Matemática WCAG ---------------------------------------------------------------- */

function rgb(hex) {
  const h = hex.replace('#', '').trim();
  const full = h.length === 3 ? h.split('').map((c) => c + c).join('') : h;
  return [0, 2, 4].map((i) => parseInt(full.slice(i, i + 2), 16));
}

function channel(v) {
  const s = v / 255;
  return s <= 0.03928 ? s / 12.92 : ((s + 0.055) / 1.055) ** 2.4;
}

function luminance(hex) {
  const [r, g, b] = rgb(hex);
  return 0.2126 * channel(r) + 0.7152 * channel(g) + 0.0722 * channel(b);
}

function ratio(fg, bg) {
  const [a, b] = [luminance(fg), luminance(bg)].sort((x, y) => y - x);
  return (a + 0.05) / (b + 0.05);
}

/** Aproxima `filter: brightness(f)` de CSS (multiplica los canales sRGB). */
function brighten(hex, f) {
  const h = rgb(hex)
    .map((v) => Math.min(255, Math.round(v * f)))
    .map((v) => v.toString(16).padStart(2, '0'))
    .join('');
  return `#${h}`;
}

const fmt = (n) => n.toFixed(2).replace('.', ',');
const clampText = (v) => (v >= 4.5 ? 'AA' : v >= 3 ? 'AA grande' : 'FALLA');

/* ---- Pares que se miden ------------------------------------------------------------- */

/** Colores de cada marca de proveedor: los dos extremos del degradado. */
function providerStops() {
  const out = {};
  const re = /\[data-provider='([\w-]+)'\]\s*\{\s*background:\s*linear-gradient\(180deg,\s*(#[0-9a-f]{3,8}),\s*(#[0-9a-f]{3,8})\)/gi;
  for (const m of providerSrc.matchAll(re)) out[m[1]] = [m[2], m[3]];
  return out;
}

const TEXT = 4.5; // AA texto normal
const UI = 3; // AA componentes/gráficos

function pairs(tokens) {
  const rows = [];
  const add = (label, fg, bg, min = TEXT) => {
    const r = ratio(fg, bg);
    rows.push({ label, fg, bg, ratio: r, min, ok: r >= min });
  };

  // Texto sobre superficie elevada (tarjetas, campos, filas).
  for (const surface of ['bg-raised', 'bg-float', 'bg-subtle']) {
    for (const text of ['text', 'text-muted', 'text-subtle']) {
      add(`${text} sobre ${surface}`, tokens[text], tokens[surface]);
    }
  }

  // Botón primario: color plano y los dos extremos del degradado.
  add('accent-fg sobre accent (plano)', tokens['accent-fg'], tokens['accent']);
  add('accent-fg sobre accent-hover', tokens['accent-fg'], tokens['accent-hover']);
  add('accent-fg sobre grad-from (degradado)', tokens['accent-fg'], tokens['accent-grad-from']);
  add('accent-fg sobre grad-to (degradado)', tokens['accent-fg'], tokens['accent-grad-to']);
  // El hover del primario aplica `filter: brightness(1.07)`: el peor caso del degradado.
  add('accent-fg sobre grad-from al pasar (hover)', tokens['accent-fg'], brighten(tokens['accent-grad-from'], 1.07));

  // Marcas de proveedor: iniciales blancas sobre el degradado.
  for (const [name, [from, to]] of Object.entries(providerStops())) {
    add(`blanco sobre marca ${name} (arriba)`, '#ffffff', from);
    add(`blanco sobre marca ${name} (abajo)`, '#ffffff', to);
  }

  // Contorno de campos y foco (componentes: 3:1).
  add('border-strong sobre bg-surface', tokens['border-strong'], tokens['bg-surface'], UI);
  add('focus-ring sobre bg-surface', tokens['focus-ring'], tokens['bg-surface'], UI);
  return rows;
}

/* ---- Informe ------------------------------------------------------------------------ */

const themes = [
  ['Claro', light],
  ['Oscuro', dark]
];

const all = themes.map(([name, tokens]) => ({ name, rows: pairs(tokens) }));
const failures = all.flatMap((t) => t.rows.filter((r) => !r.ok));

if (process.argv.includes('--md')) {
  for (const { name, rows } of all) {
    console.log(`\n### Tema ${name.toLowerCase()}\n`);
    console.log('| Par | Primer plano | Fondo | Ratio | Mínimo | Resultado |');
    console.log('|-----|--------------|-------|------:|-------:|-----------|');
    for (const r of rows) {
      console.log(`| ${r.label} | \`${r.fg}\` | \`${r.bg}\` | ${fmt(r.ratio)}:1 | ${r.min}:1 | ${r.ok ? '✔' : '✘'} |`);
    }
  }
} else {
  for (const { name, rows } of all) {
    console.log(`\n${name}`);
    console.log('─'.repeat(72));
    for (const r of rows) {
      console.log(
        `${r.ok ? '✔' : '✘'} ${r.label.padEnd(44)} ${fmt(r.ratio).padStart(5)}:1  (mín ${r.min})`
      );
    }
  }
  console.log(
    `\n${failures.length} de ${all.reduce((n, t) => n + t.rows.length, 0)} pares por debajo de AA.`
  );
}

if (failures.length > 0) {
  console.log('\nPares que fallan:');
  for (const f of failures) console.log(` - ${f.label}: ${fmt(f.ratio)}:1 (mín ${f.min}:1)`);
  if (process.argv.includes('--fail')) process.exitCode = 1;
}
