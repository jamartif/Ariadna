import { describe, expect, it } from 'vitest';
import type { StatusEntry } from '$lib/api/types';
import { buildTreeRows } from './tree';

/** Entrada de estado mínima: el árbol solo mira la ruta. */
function entry(path: string): StatusEntry {
  return {
    path,
    originalPath: null,
    indexStatus: 'M',
    workTreeStatus: ' ',
    isConflicted: false,
    isUntracked: false,
    isIgnored: false,
    isSubmodule: false,
    conflictKind: null
  } as unknown as StatusEntry;
}

const shape = (rows: ReturnType<typeof buildTreeRows>) =>
  rows.map((r) =>
    r.type === 'dir'
      ? `${'  '.repeat(r.depth)}[${r.name}]`
      : `${'  '.repeat(r.depth)}${String(r.entry.path).split('/').pop()}`
  );

/**
 * El modo árbol tiene que anidar cada nivel de la ruta: con `ui/src/lib/util/branchTree.ts` se ven
 * `ui` › `src` › `lib` › `util` y el fichero debajo, no una fila compactada ni una lista plana.
 */
describe('buildTreeRows', () => {
  const changes = [entry('README.md'), entry('ui/src/App.svelte'), entry('ui/src/lib/util/branchTree.ts')];

  it('muestra un nodo por nivel de la ruta y deja el fichero raíz suelto', () => {
    expect(shape(buildTreeRows('unstaged', changes, new Set()))).toEqual([
      'README.md',
      '[ui]',
      '  [src]',
      '    App.svelte',
      '    [lib]',
      '      [util]',
      '        branchTree.ts'
    ]);
  });

  it('cada carpeta sabe qué ficheros cuelgan de su subárbol', () => {
    const rows = buildTreeRows('unstaged', changes, new Set());
    const dir = (name: string) => rows.find((r) => r.type === 'dir' && r.name === name);
    const ui = dir('ui');
    expect(ui && ui.type === 'dir' ? ui.entries.map((e) => String(e.path)) : []).toEqual([
      'ui/src/App.svelte',
      'ui/src/lib/util/branchTree.ts'
    ]);
    const util = dir('util');
    expect(util && util.type === 'dir' ? util.entries : []).toHaveLength(1);
    // `README.md` está en la raíz: no se inventa carpeta intermedia ni entrada vacía.
    expect(rows.some((r) => r.type === 'dir' && r.name === '')).toBe(false);
    expect(rows.filter((r) => r.type === 'file')).toHaveLength(3);
  });

  it('no cambia el orden de los ficheros respecto al modo lista', () => {
    const listed = changes.map((e) => String(e.path));
    const fromTree = buildTreeRows('unstaged', changes, new Set())
      .filter((r) => r.type === 'file')
      .map((r) => String(r.entry.path));
    expect(fromTree).toEqual(listed);
  });

  it('plegar una carpeta oculta su subárbol y deja la fila de la carpeta', () => {
    expect(shape(buildTreeRows('unstaged', changes, new Set(['unstaged:ui/src'])))).toEqual([
      'README.md',
      '[ui]',
      '  [src]'
    ]);
  });
});
