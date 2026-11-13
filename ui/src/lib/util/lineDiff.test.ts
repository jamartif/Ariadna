import { describe, expect, it } from 'vitest';
import { diffStats, lineDiff, type DiffRow } from './lineDiff';

/** Texto de una fila, con su signo, para comparar de un vistazo. */
const show = (rows: DiffRow[]) =>
  rows.filter((r) => r.kind !== 'same').map((r) => `${r.kind}:${r.text}`);

describe('lineDiff', () => {
  it('sin cambios no hay filas de cambio', () => {
    const rows = lineDiff('uno\ndos\ntres\n', 'uno\ndos\ntres\n');
    expect(show(rows)).toEqual([]);
    expect(rows).toHaveLength(4);
    expect(diffStats(rows)).toEqual({ additions: 0, deletions: 0 });
  });

  it('una línea cambiada se ve como quitar y añadir', () => {
    const rows = lineDiff('uno\ndos\ntres\n', 'uno\nDOS\ntres\n');
    expect(show(rows)).toEqual(['del:dos', 'add:DOS']);
    expect(diffStats(rows)).toEqual({ additions: 1, deletions: 1 });
  });

  it('añadir al final y quitar al principio', () => {
    expect(show(lineDiff('a\nb\n', 'a\nb\nc\n'))).toEqual(['add:c']);
    expect(show(lineDiff('a\nb\nc\n', 'b\nc\n'))).toEqual(['del:a']);
  });

  it('recorta el contexto y resume los huecos en una sola fila', () => {
    const before = Array.from({ length: 40 }, (_, i) => `l${i}`).join('\n');
    const after = before.replace('l0', 'CERO').replace('l39', 'FIN');
    const rows = lineDiff(before, after, 1);
    expect(rows.filter((r) => r.kind === 'gap')).toHaveLength(1);
    expect(rows.length).toBeLessThan(12);
    expect(show(rows)).toEqual(['del:l0', 'add:CERO', 'gap:', 'del:l39', 'add:FIN']);
    // Las líneas de contexto que quedan son las de al lado de cada cambio.
    expect(rows.filter((r) => r.kind === 'same').map((r) => r.text)).toEqual(['l1', 'l38']);
  });

  it('textos vacíos y un solo lado', () => {
    expect(show(lineDiff('', 'x\n'))).toEqual(['add:x']);
    expect(show(lineDiff('x\n', ''))).toEqual(['del:x']);
    expect(show(lineDiff('', ''))).toEqual([]);
  });
});
