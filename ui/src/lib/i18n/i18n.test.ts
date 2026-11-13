// @vitest-environment node
import { readdirSync, readFileSync, statSync } from 'node:fs';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';
import { t, tn } from './index';
import { es } from './es';
import { en } from './en';
import { esScreens } from './es.screens';
import { enScreens } from './en.screens';
import { esPr } from './es.pr';
import { enPr } from './en.pr';
import { esSettings } from './es.settings';
import { enSettings } from './en.settings';
import { esConflicts } from './es.conflicts';
import { enConflicts } from './en.conflicts';
import { esRemotes } from './es.remotes';
import { enRemotes } from './en.remotes';
import { esStash } from './es.stash';
import { enStash } from './en.stash';
import { esTags } from './es.tags';
import { enTags } from './en.tags';

/** Fuera de la lista: los propios catálogos (`es*`, `en*`) y los tests. */
const CATALOG_FILE = /^(es|en)(\..+)?\.ts$/;

function sources(dir: string): string[] {
  const out: string[] = [];
  for (const name of readdirSync(dir)) {
    const p = join(dir, name);
    if (statSync(p).isDirectory()) out.push(...sources(p));
    else if (/\.(svelte|ts)$/.test(name) && !/\.test\.ts$/.test(name) && !CATALOG_FILE.test(name)) out.push(p);
  }
  return out;
}

// `fileURLToPath` (y no `.pathname`) para que la ruta sea válida también en Windows (`/C:/…`).
const root = fileURLToPath(new URL('../../', import.meta.url));
const code = sources(root).map((p) => ({ p, text: readFileSync(p, 'utf8') }));
const keys = Object.keys(es) as (keyof typeof es)[];

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

/** Nombres de los parámetros `{x}` de una plantilla, ordenados. */
const params = (value: string): string[] => [...new Set([...value.matchAll(/\{(\w+)\}/g)].map((m) => m[1] as string))].sort();

const CATALOGS: [string, Record<string, string>, Record<string, string>][] = [
  ['base', es, en],
  ['screens', esScreens, enScreens],
  ['pr', esPr, enPr],
  ['settings', esSettings, enSettings],
  ['conflicts', esConflicts, enConflicts],
  ['remotes', esRemotes, enRemotes],
  ['stash', esStash, enStash],
  ['tags', esTags, enTags]
];

describe('paridad es ↔ en', () => {
  it('el catálogo inglés tiene exactamente las mismas claves que el español', () => {
    expect(keys.filter((k) => !(k in en))).toEqual([]);
    expect(Object.keys(en).filter((k) => !(k in es))).toEqual([]);
    expect(Object.keys(en)).toHaveLength(keys.length);
  });

  it('cada archivo de dominio traduce el mismo conjunto de claves', () => {
    for (const [name, esCat, enCat] of CATALOGS) {
      const a = Object.keys(esCat).sort();
      const b = Object.keys(enCat).sort();
      expect(b.filter((k) => !a.includes(k)), `${name}: claves de más en inglés`).toEqual([]);
      expect(a.filter((k) => !b.includes(k)), `${name}: claves sin traducir`).toEqual([]);
      expect(b, `${name}: número de claves`).toHaveLength(a.length);
    }
  });

  it('los plurales (.one/.other) y los parámetros {x} son los mismos en ambos idiomas', () => {
    const badParams = keys.filter((k) => params(es[k]!).join() !== params(en[k]!).join());
    const badPlurals = keys.filter((k) => k.endsWith('.one') && !(`${k.replace(/\.one$/, '.other')}` in en));
    expect(badParams).toEqual([]);
    expect(badPlurals).toEqual([]);
  });

  it('ninguna traducción inglesa está vacía', () => {
    expect(Object.entries(en).filter(([, v]) => v.trim() === '').map(([k]) => k)).toEqual([]);
  });

  it('el inglés no arrastra el texto español (salvo marcas y nombres propios)', () => {
    // Claves que se escriben igual en los dos idiomas a propósito: marcas, siglas, nombres de
    // proveedor, rutas de ejemplo y palabras que la interfaz usa tal cual. Si aparece una nueva,
    // este test obliga a decidir si es una traducción pendiente o también es igual en inglés.
    const sameOnPurpose = new Set([
      'ai.dialog.profile.personal',
      'clone.viaHttps',
      'clone.viaSsh',
      'clone.provider.azure',
      'clone.provider.github',
      'clone.provider.gitlab',
      'clone.provider.bitbucket',
      'clone.token',
      'history.commit',
      'panel.pullRequests',
      'pr.region',
      'pr.count.one',
      'pr.count.other',
      'pr.fork',
      'pr.detail.commits',
      'pr.strategy.squash',
      'pr.strategy.rebase',
      'settings.section.general',
      'settings.section.git',
      'settings.general.languageEs',
      'settings.general.languageEn',
      'settings.accounts.write.azure',
      'settings.accounts.write.bitbucket',
      'settings.accounts.azure',
      'settings.accounts.github',
      'settings.accounts.gitlab',
      'settings.accounts.bitbucket',
      'settings.about.licenseValue',
      'conflicts.base',
      'remotes.namePlaceholder',
      'remotes.ssh',
      'remotes.provider.local',
      'panel.stashes',
      'sidebar.stashes',
      'stash.chip.one',
      'stash.chip.other',
      'stash.commit',
      'shortcut.key.tab',
      'tags.namePlaceholder'
    ]);
    expect(keys.filter((k) => es[k] === en[k] && !sameOnPurpose.has(k))).toEqual([]);
  });
});
