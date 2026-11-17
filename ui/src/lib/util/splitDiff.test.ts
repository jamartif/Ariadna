//! `splitDiff`: la misma información que la vista unificada, colocada en dos columnas.

import { describe, expect, it } from 'vitest';

import type { DiffHunk, DiffLine, FileDiff } from '$lib/api/types';
import { splitDiff } from './diff';

let n = 0;
const line = (kind: DiffLine['kind'], text: string, oldNum: number | null, newNum: number | null): DiffLine => ({
  id: `l${n++}`,
  kind,
  text,
  oldLineNumber: oldNum,
  newLineNumber: newNum
});

function diffDe(lines: DiffLine[]): FileDiff {
  const hunk: DiffHunk = {
    id: 'h1',
    oldStart: 1,
    oldCount: 3,
    newStart: 1,
    newCount: 3,
    heading: '',
    lines
  };
  return { path: 'a.txt', hunks: [hunk], isBinary: false, isNewFile: false, isDeletedFile: false } as FileDiff;
}

describe('splitDiff', () => {
  it('las líneas iguales van en las dos columnas', () => {
    const filas = splitDiff(diffDe([line('context', 'igual', 1, 1)]));
    const par = filas.find((f) => f.type === 'pair');
    expect(par).toMatchObject({ left: expect.objectContaining({ text: 'igual' }), right: expect.objectContaining({ text: 'igual' }) });
  });

  it('empareja borradas con añadidas en el mismo orden', () => {
    const filas = splitDiff(
      diffDe([
        line('deletion', 'viejo 1', 1, null),
        line('deletion', 'viejo 2', 2, null),
        line('addition', 'nuevo 1', null, 1),
        line('addition', 'nuevo 2', null, 2)
      ])
    );
    const pares = filas.filter((f) => f.type === 'pair');
    expect(pares).toHaveLength(2);
    expect(pares[0]).toMatchObject({ left: expect.objectContaining({ text: 'viejo 1' }), right: expect.objectContaining({ text: 'nuevo 1' }) });
    expect(pares[1]).toMatchObject({ left: expect.objectContaining({ text: 'viejo 2' }), right: expect.objectContaining({ text: 'nuevo 2' }) });
  });

  it('si un lado tiene más líneas, las sobrantes salen con el otro lado vacío', () => {
    const filas = splitDiff(
      diffDe([
        line('deletion', 'viejo 1', 1, null),
        line('addition', 'nuevo 1', null, 1),
        line('addition', 'nuevo 2', null, 2)
      ])
    );
    const pares = filas.filter((f) => f.type === 'pair');
    expect(pares).toHaveLength(2);
    expect(pares[1]).toMatchObject({ left: null, right: expect.objectContaining({ text: 'nuevo 2' }) });
  });

  it('cada hunk abre con su cabecera', () => {
    const filas = splitDiff(diffDe([line('context', 'x', 1, 1)]));
    expect(filas[0]).toMatchObject({ type: 'hunk', hunkIndex: 0 });
  });

  it('no pierde ninguna línea de la vista unificada', () => {
    const lineas = [
      line('context', 'a', 1, 1),
      line('deletion', 'b', 2, null),
      line('addition', 'B', null, 2),
      line('context', 'c', 3, 3)
    ];
    const pares = splitDiff(diffDe(lineas)).filter((f) => f.type === 'pair');
    const textos = pares.flatMap((p) => [p.type === 'pair' ? p.left?.text : null, p.type === 'pair' ? p.right?.text : null]).filter(Boolean);
    expect(textos).toContain('a');
    expect(textos).toContain('b');
    expect(textos).toContain('B');
    expect(textos).toContain('c');
  });
});
