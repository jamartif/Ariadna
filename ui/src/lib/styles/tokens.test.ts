// @vitest-environment node
import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';

const css = readFileSync(new URL('./tokens.css', import.meta.url), 'utf8');

/** Extrae el cuerpo del bloque cuyo selector empieza por `selector`. */
function block(selector: string, from = 0): { body: string; end: number } {
  const start = css.indexOf(selector, from);
  if (start < 0) throw new Error(`No se encontró ${selector}`);
  const open = css.indexOf('{', start);
  let depth = 0;
  for (let i = open; i < css.length; i++) {
    if (css[i] === '{') depth++;
    else if (css[i] === '}' && --depth === 0) return { body: css.slice(open + 1, i), end: i };
  }
  throw new Error('bloque sin cerrar');
}

function vars(body: string): Record<string, string> {
  const out: Record<string, string> = {};
  for (const m of body.matchAll(/--([\w-]+):\s*([^;]+);/g)) out[m[1] as string] = (m[2] as string).trim();
  return out;
}

const light = vars(block(':root {').body);
const dark = vars(block(":root[data-theme='dark']").body);
const media = block('@media (prefers-color-scheme: dark)').body;
const systemDark = vars(block(":root:not([data-theme='light'])", css.indexOf('@media (prefers-color-scheme: dark)')).body);

function channel(c: number): number {
  const s = c / 255;
  return s <= 0.03928 ? s / 12.92 : ((s + 0.055) / 1.055) ** 2.4;
}
function luminance(hex: string): number {
  const h = hex.replace('#', '');
  const [r, g, b] = [0, 2, 4].map((i) => parseInt(h.slice(i, i + 2), 16));
  return 0.2126 * channel(r as number) + 0.7152 * channel(g as number) + 0.0722 * channel(b as number);
}
export function contrast(a: string, b: string): number {
  const [hi, lo] = [luminance(a), luminance(b)].sort((x, y) => y - x);
  return ((hi as number) + 0.05) / ((lo as number) + 0.05);
}

const themes = { claro: light, oscuro: dark } as const;

describe('tokens de diseño (spec §7.1)', () => {
  it('el tema oscuro del sistema es idéntico al oscuro explícito', () => {
    expect(systemDark).toEqual(dark);
    expect(media).toContain('--bg-app: #080b10');
  });

  it('ambos temas definen exactamente las mismas variables de color', () => {
    expect(Object.keys(dark).sort()).toEqual(Object.keys(light).sort());
  });

  it('escala de espaciado de 4/8 px', () => {
    const all = vars(block(':root {', css.indexOf('Tokens independientes')).body);
    const spaces = Object.entries(all).filter(([k]) => k.startsWith('space-'));
    expect(spaces.length).toBeGreaterThanOrEqual(6);
    for (const [k, v] of spaces) expect(parseInt(v) % 4, `${k}=${v}`).toBe(0);
  });

  it('exactamente 3 radios y las elevaciones 1 y 2 (más el reborde interior)', () => {
    const all = vars(block(':root {', css.indexOf('Tokens independientes')).body);
    expect(Object.keys(all).filter((k) => k.startsWith('radius-'))).toHaveLength(3);
    expect(Object.keys(light).filter((k) => k.startsWith('elev-')).sort()).toEqual(['elev-1', 'elev-2', 'elev-inset']);
  });

  it('el movimiento está entre 120 y 180 ms', () => {
    const all = vars(block(':root {', css.indexOf('Tokens independientes')).body);
    expect(parseInt(all['dur-fast'] as string)).toBe(120);
    expect(parseInt(all['dur-slow'] as string)).toBe(180);
  });

  it('las superficies se apilan: app < panel < elevado < flotante (más claras en oscuro)', () => {
    const lum = (k: string) => luminance(dark[k] as string);
    expect(lum('bg-app')).toBeLessThan(lum('bg-surface'));
    expect(lum('bg-surface')).toBeLessThan(lum('bg-raised'));
    expect(lum('bg-raised')).toBeLessThan(lum('bg-float'));
  });

  it('la transición de tema es ≤ 150 ms', () => {
    const all = vars(block(':root {', css.indexOf('Tokens independientes')).body);
    expect(parseInt(all['dur'] as string)).toBeLessThanOrEqual(150);
  });

  it('el acento es el cian del icono', () => {
    expect(light['accent-brand']).toBe('#06b6d4');
    expect(dark['accent']).toBe('#22d3ee');
  });

  for (const [name, t] of Object.entries(themes)) {
    describe(`contraste en tema ${name}`, () => {
      const surfaces = ['bg-app', 'bg-surface', 'bg-raised', 'bg-float', 'bg-subtle', 'bg-muted', 'bg-selected'];
      const textPairs: [string, string[]][] = [
        ['text', surfaces],
        ['text-muted', surfaces],
        ['text-subtle', surfaces],
        ['accent', ['bg-surface', 'bg-app', 'accent-soft']],
        ['success', ['bg-surface', 'success-soft']],
        ['danger', ['bg-surface', 'danger-soft']],
        ['warning', ['bg-surface', 'warning-soft']],
        ['info', ['bg-surface', 'info-soft']],
        ['text', ['diff-add-bg', 'diff-del-bg', 'diff-hunk-bg']],
        ['text-muted', ['diff-add-gutter', 'diff-del-gutter', 'diff-hunk-bg', 'bg-subtle']],
        ['success', ['diff-add-bg']],
        ['danger', ['diff-del-bg']],
        ['accent-fg', ['accent', 'accent-hover', 'accent-grad-from', 'accent-grad-to']],
        ['text', ['diff-add-strong', 'diff-del-strong']],
        ['tok-comment', ['bg-surface', 'diff-add-bg', 'diff-del-bg']],
        ['tok-string', ['bg-surface', 'diff-add-bg', 'diff-del-bg']],
        ['tok-keyword', ['bg-surface', 'diff-add-bg', 'diff-del-bg']],
        ['tok-function', ['bg-surface', 'diff-add-bg', 'diff-del-bg']],
        ['tok-number', ['bg-surface', 'diff-add-bg', 'diff-del-bg']],
        ['tok-type', ['bg-surface', 'diff-add-bg', 'diff-del-bg']],
        ['tok-property', ['bg-surface', 'diff-add-bg', 'diff-del-bg']],
        ['tok-operator', ['bg-surface', 'diff-add-bg', 'diff-del-bg']],
        ['tok-punct', ['bg-surface', 'diff-add-bg', 'diff-del-bg']],
        ['tok-tag', ['bg-surface', 'diff-add-bg', 'diff-del-bg']],
        ['tok-regex', ['bg-surface', 'diff-add-bg', 'diff-del-bg']],
        ['st-added-fg', ['st-added-bg']],
        ['st-modified-fg', ['st-modified-bg']],
        ['st-deleted-fg', ['st-deleted-bg']],
        ['st-renamed-fg', ['st-renamed-bg']],
        ['st-unmerged-fg', ['st-unmerged-bg']],
        ['st-untracked-fg', ['st-untracked-bg']]
      ];
      for (const [fg, bgs] of textPairs) {
        for (const bg of bgs) {
          it(`texto ${fg} sobre ${bg} ≥ 4,5:1`, () => {
            expect(contrast(t[fg] as string, t[bg] as string), `${t[fg]} sobre ${t[bg]}`).toBeGreaterThanOrEqual(4.5);
          });
        }
      }
      const uiPairs: [string, string[]][] = [
        ['border-strong', ['bg-surface', 'bg-app', 'bg-subtle']],
        ['focus-ring', ['bg-surface', 'bg-app', 'bg-selected']],
        ['accent', ['bg-surface', 'bg-app']]
      ];
      for (const [fg, bgs] of uiPairs) {
        for (const bg of bgs) {
          it(`elemento ${fg} sobre ${bg} ≥ 3:1`, () => {
            expect(contrast(t[fg] as string, t[bg] as string), `${t[fg]} sobre ${t[bg]}`).toBeGreaterThanOrEqual(3);
          });
        }
      }
    });
  }
});
