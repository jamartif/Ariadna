// @vitest-environment node
import { readdirSync, readFileSync, statSync } from 'node:fs';
import { join } from 'node:path';
import { describe, expect, it } from 'vitest';
import { t, tn } from './index';
import { es } from './es';

function sources(dir: string): string[] {
  const out: string[] = [];
  for (const name of readdirSync(dir)) {
    const p = join(dir, name);
    if (statSync(p).isDirectory()) out.push(...sources(p));
    else if (/\.(svelte|ts)$/.test(name) && !/\.test\.ts$/.test(name) && name !== 'es.ts') out.push(p);
  }
  return out;
}

const root = new URL('../../', import.meta.url).pathname;
const code = sources(root).map((p) => ({ p, text: readFileSync(p, 'utf8') }));
const keys = Object.keys(es);

describe('catálogo i18n', () => {
  it('toda clave literal usada en el código existe en es.ts', () => {
    const missing: string[] = [];
    for (const { p, text } of code) {
      for (const m of text.matchAll(/\bt\(\s*'([a-zA-Z0-9_.]+)'/g)) {
        if (!(m[1] as string in es)) missing.push(`${p}: ${m[1]}`);
      }
      for (const m of text.matchAll(/\btn\(\s*'([a-zA-Z0-9_.]+)'/g)) {
        for (const suffix of ['one', 'other']) if (!(`${m[1]}.${suffix}` in es)) missing.push(`${p}: ${m[1]}.${suffix}`);
      }
      for (const m of text.matchAll(/(?:label|labelKey):\s*'([a-zA-Z0-9_.]+)'/g)) {
        if (!(m[1] as string in es)) missing.push(`${p}: ${m[1]}`);
      }
    }
    expect(missing).toEqual([]);
  });

  it('las claves dinámicas cubren todos sus valores', () => {
    for (const k of ['bothModified', 'bothAdded', 'bothDeleted', 'addedByUs', 'addedByThem', 'deletedByUs', 'deletedByThem', 'unknown']) {
      expect(es).toHaveProperty([`conflict.${k}`]);
    }
    for (const k of ['none', 'merging', 'rebasing', 'cherryPicking']) expect(es).toHaveProperty([`integration.${k}`]);
    for (const k of ['system', 'light', 'dark']) expect(es).toHaveProperty([`settings.theme.${k}`]);
  });

  it('cada plural tiene .one y .other', () => {
    for (const k of keys.filter((x) => x.endsWith('.one'))) expect(es).toHaveProperty([k.replace(/\.one$/, '.other')]);
    for (const k of keys.filter((x) => x.endsWith('.other'))) expect(es).toHaveProperty([k.replace(/\.other$/, '.one')]);
  });

  it('interpola y pluraliza', () => {
    expect(t('tabs.close', { name: 'repo' })).toBe('Cerrar repo');
    expect(tn('commit.staged', 1)).toBe('1 archivo preparado');
    expect(tn('commit.staged', 3)).toBe('3 archivos preparados');
    expect(t('time.minutes', { n: 5 })).toBe('hace 5 min');
  });

  it('no hay texto de interfaz español incrustado en los componentes', () => {
    // Heurística: texto visible entre etiquetas con letras acentuadas o «ñ» fuera de t()/comentarios.
    const offenders: string[] = [];
    for (const { p, text } of code.filter((c) => c.p.endsWith('.svelte'))) {
      const markup = text
        .replace(/<script[\s\S]*?<\/script>/g, '')
        .replace(/<style[\s\S]*?<\/style>/g, '')
        .replace(/<!--[\s\S]*?-->/g, '')
        .replace(/\{[^}]*\}/g, '');
      for (const m of markup.matchAll(/>([^<>]*[áéíóúñ¿¡][^<>]*)</g)) offenders.push(`${p}: ${m[1]?.trim()}`);
    }
    expect(offenders).toEqual([]);
  });
});
