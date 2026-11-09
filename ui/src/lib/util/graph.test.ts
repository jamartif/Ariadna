import { describe, expect, it } from 'vitest';
import { richHistory } from '$lib/api/mockApi';
import { emptyGraphState, layoutGraph, maxWidth, type GraphCommit } from './graph';

const c = (sha: string, ...parents: string[]): GraphCommit => ({ sha, parents });

describe('grafo de ramas', () => {
  it('una historia lineal ocupa una sola columna', () => {
    const { rows } = layoutGraph([c('c', 'b'), c('b', 'a'), c('a')]);
    expect(rows.map((r) => r.lane)).toEqual([0, 0, 0]);
    expect(rows[0]).toMatchObject({ incoming: false, parents: [0], joins: [], through: [] });
    expect(rows[1]).toMatchObject({ incoming: true, parents: [0] });
    expect(rows[2]).toMatchObject({ incoming: true, parents: [] });
    expect(maxWidth(rows)).toBe(1);
  });

  it('un merge abre una segunda columna que vuelve a unirse en el ancestro común', () => {
    //  m ─┬─ a ─┐        m: merge de a (1.º padre) y s (2.º)
    //     └─ s ─┤        a y s comparten padre `base`
    const { rows } = layoutGraph([c('m', 'a', 's'), c('a', 'base'), c('s', 'base'), c('base')]);
    expect(rows[0]).toMatchObject({ lane: 0, parents: [0, 1] });
    expect(rows[1]).toMatchObject({ lane: 0, through: [1], parents: [0] });
    // `s` (columna 1) ve que `base` ya espera en la columna 0 y se curva hacia ella.
    expect(rows[2]).toMatchObject({ lane: 1, through: [0], incoming: true, parents: [0] });
    expect(rows[3]).toMatchObject({ lane: 0, incoming: true, parents: [] });
    expect(maxWidth(rows)).toBe(2);
  });

  it('dos puntas de rama distintas usan columnas distintas', () => {
    const { rows } = layoutGraph([c('x', 'r'), c('y', 'r'), c('r')]);
    expect(rows[0]?.lane).toBe(0);
    expect(rows[1]?.lane).toBe(1);
    expect(rows[1]?.through).toEqual([0]);
    expect(rows[1]?.parents).toEqual([0]);
    expect(rows[2]).toMatchObject({ lane: 0, incoming: true });
    expect(rows[2]?.through).toEqual([]);
  });

  it('paginar da exactamente las mismas filas que calcularlo todo de una vez', () => {
    const all = richHistory(137).map((x) => ({ sha: x.sha, parents: x.parents }));
    const whole = layoutGraph(all).rows;
    let state = emptyGraphState();
    const paged = [];
    for (let i = 0; i < all.length; i += 25) {
      const out = layoutGraph(all.slice(i, i + 25), state);
      state = out.state;
      paged.push(...out.rows);
    }
    expect(paged).toEqual(whole);
  });

  it('el historial sintético es un DAG ordenado y el grafo no se dispara en anchura', () => {
    const list = richHistory(500);
    const pos = new Map(list.map((x, i) => [x.sha, i]));
    for (const [i, x] of list.entries()) for (const p of x.parents) expect(pos.get(p)!).toBeGreaterThan(i);
    const rows = layoutGraph(list).rows;
    expect(maxWidth(rows)).toBeLessThanOrEqual(4);
    // Cada fila coloca su nodo en una columna que existe.
    for (const r of rows) expect(r.lane).toBeLessThan(r.width);
  });
});
