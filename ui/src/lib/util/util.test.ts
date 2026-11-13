import { describe, expect, it } from 'vitest';
import type { FileDiff, StatusEntry } from '$lib/api/types';
import { fuzzyFilter, fuzzyMatch } from './fuzzy';
import { groupEntries, displayStatus } from './changes';
import { GIANT_LINES, countLines, diffStats, flattenDiff, isGiant, truncateLine, LONG_LINE } from './diff';
import { pathBase, pathDir, pathEq, pathKey, pathText } from './gitpath';
import { buildOffsets, indexAt, moveItem, visibleRange } from './virtual';
import { relativeTime } from './time';
import { makeHunk } from '$lib/api/mockData';

describe('fuzzy', () => {
  it('coincide por subsecuencia sin importar acentos ni mayúsculas', () => {
    expect(fuzzyMatch('pest', 'Nueva pestaña')).not.toBeNull();
    expect(fuzzyMatch('tema osc', 'Tema: oscuro')).not.toBeNull();
    expect(fuzzyMatch('xyz', 'Nueva pestaña')).toBeNull();
    expect(fuzzyMatch('', 'cualquiera')).toEqual({ score: 0, positions: [] });
  });

  it('puntúa mejor el inicio de palabra y las coincidencias seguidas', () => {
    const r = fuzzyFilter('ca', ['Cerrar pestaña', 'Cambiar densidad', 'Preparar todo'], (s) => s).map((x) => x.item);
    expect(r[0]).toBe('Cambiar densidad');
    expect(r).not.toContain('Preparar todo');
  });
});

describe('virtual', () => {
  it('calcula la ventana visible con alturas variables', () => {
    const o = buildOffsets(1000, (i) => (i % 10 === 0 ? 30 : 20));
    expect(o[1000]).toBe(100 * 30 + 900 * 20);
    expect(indexAt(o, 0)).toBe(0);
    expect(indexAt(o, 29)).toBe(0);
    expect(indexAt(o, 30)).toBe(1);
    const { start, end } = visibleRange(o, 5000, 400, 3);
    expect(end - start).toBeLessThan(30);
    expect(start).toBeGreaterThan(200);
    expect(visibleRange(buildOffsets(0, () => 1), 0, 100, 3)).toEqual({ start: 0, end: 0 });
  });

  it('moveItem acota y no muta', () => {
    const src = ['a', 'b', 'c'];
    expect(moveItem(src, 0, 1)).toEqual(['b', 'a', 'c']);
    expect(moveItem(src, 2, 5)).toEqual(['a', 'b', 'c']);
    expect(moveItem(src, 2, -1)).toEqual(['a', 'c', 'b']);
    expect(src).toEqual(['a', 'b', 'c']);
  });
});

describe('gitpath', () => {
  it('acepta cadenas y bytes no UTF-8 sin tocarlos', () => {
    const bytes = [0x61, 0xff, 0x2f, 0x62];
    expect(pathKey(bytes)).toBe('bytes:97,255,47,98');
    expect(pathEq(bytes, [...bytes])).toBe(true);
    expect(pathEq('a', bytes)).toBe(false);
    expect(pathText('src/a.ts')).toBe('src/a.ts');
    expect(pathText(bytes)).toContain('\uFFFD');
    expect(pathBase('src/lib/a.ts')).toBe('a.ts');
    expect(pathDir('src/lib/a.ts')).toBe('src/lib');
    expect(pathDir('a.ts')).toBe('');
  });
});

function entry(over: Partial<StatusEntry>): StatusEntry {
  return {
    path: 'x',
    originalPath: null,
    indexStatus: '.',
    workTreeStatus: '.',
    isConflicted: false,
    isUntracked: false,
    isIgnored: false,
    isSubmodule: false,
    conflictKind: null,
    ...over
  };
}

describe('changes', () => {
  it('replica los predicados de Rust: un archivo puede estar en dos secciones', () => {
    const g = groupEntries([
      entry({ path: 'both', indexStatus: 'M', workTreeStatus: 'M' }),
      entry({ path: 'staged', indexStatus: 'A' }),
      entry({ path: 'wt', workTreeStatus: 'D' }),
      entry({ path: 'new', isUntracked: true, indexStatus: '?', workTreeStatus: '?' }),
      entry({ path: 'ign', isIgnored: true, workTreeStatus: '!' }),
      entry({ path: 'conf', isConflicted: true, indexStatus: 'U', workTreeStatus: 'U' })
    ]);
    expect(g.staged.map((e) => e.path)).toEqual(['both', 'staged']);
    expect(g.unstaged.map((e) => e.path)).toEqual(['both', 'wt']);
    expect(g.untracked.map((e) => e.path)).toEqual(['new']);
    expect(g.ignored.map((e) => e.path)).toEqual(['ign']);
    expect(g.conflicts.map((e) => e.path)).toEqual(['conf']);
    expect(displayStatus(entry({ indexStatus: 'R' }), 'staged')).toBe('R');
    expect(displayStatus(entry({}), 'untracked')).toBe('?');
  });
});

describe('diff', () => {
  const small: FileDiff = {
    path: 'a',
    oldPath: null,
    isBinary: false,
    isNewFile: false,
    isDeletedFile: false,
    hunks: [makeHunk('h', 1, 1, '', [' a', '-b', '+c', '+d'])]
  };

  it('cuenta líneas y estadísticas y aplana en filas', () => {
    expect(countLines(small)).toBe(4);
    expect(diffStats(small)).toEqual({ additions: 2, deletions: 1 });
    expect(flattenDiff(small).map((r) => r.type)).toEqual(['hunk', 'line', 'line', 'line', 'line']);
    expect(small.hunks[0]).toMatchObject({ oldStart: 1, oldCount: 2, newStart: 1, newCount: 3 });
  });

  it('gigante por nº de líneas (> 5 000) o por bytes (> 1 MB)', () => {
    expect(isGiant(small)).toBe(false);
    const many = makeHunk('m', 1, 1, '', Array.from({ length: GIANT_LINES + 1 }, () => '+x'));
    expect(isGiant({ ...small, hunks: [many] })).toBe(true);
    const exact = makeHunk('e', 1, 1, '', Array.from({ length: GIANT_LINES }, () => '+x'));
    expect(isGiant({ ...small, hunks: [exact] })).toBe(false);
    const heavy = makeHunk('b', 1, 1, '', ['+' + 'y'.repeat(1_100_000)]);
    expect(isGiant({ ...small, hunks: [heavy] })).toBe(true);
  });

  it('trunca líneas largas salvo que se expandan', () => {
    const long = 'z'.repeat(LONG_LINE + 10);
    expect(truncateLine(long, false)).toEqual({ text: 'z'.repeat(LONG_LINE), truncated: true });
    expect(truncateLine(long, true)).toEqual({ text: long, truncated: false });
    expect(truncateLine('corta', false).truncated).toBe(false);
  });
});

describe('tiempo relativo', () => {
  const now = new Date('2026-11-09T12:00:00Z');
  it('formatea en español', () => {
    expect(relativeTime('2026-11-09T11:59:40Z', now)).toBe('ahora mismo');
    expect(relativeTime('2026-11-09T11:30:00Z', now)).toBe('hace 30 min');
    expect(relativeTime('2026-11-09T09:00:00Z', now)).toBe('hace 3 h');
    expect(relativeTime('2026-11-08T09:00:00Z', now)).toBe('ayer');
    expect(relativeTime('2026-11-01T09:00:00Z', now)).toBe('hace 8 días');
    expect(relativeTime('2026-08-01T09:00:00Z', now)).toBe('hace 3 meses');
    expect(relativeTime('2024-11-01T09:00:00Z', now)).toBe('hace 2 años');
    expect(relativeTime('basura', now)).toBe('');
  });
});
