import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { Commit } from '$lib/api/types';
import { loadedTab, mockApi, resetGlobals } from '../../test/helpers';
import { HistoryController } from './history.svelte';

beforeEach(resetGlobals);
afterEach(resetGlobals);

function commit(sha: string, parents: string[]): Commit {
  return { sha, parents, subject: sha, body: '', authorName: 'a', authorEmail: 'a@x', authorDateIso: '2026-01-01T00:00:00Z', refs: [] };
}

/** Historial lineal `n0 → n1 → …` (`n0` el más nuevo) servido como lo haría Rust: por `skip` o por cursor. */
function linear(n: number): Commit[] {
  return Array.from({ length: n }, (_, i) => commit(`n${i}`, i + 1 < n ? [`n${i + 1}`] : []));
}

async function controllerWith(history: Commit[], page: number, serve?: (skip: number, limit: number, cursor: string[] | null) => Commit[]) {
  const { tab } = await loadedTab('tab-ariadna');
  const calls: { skip: number; limit: number; cursor: string[] | null }[] = [];
  vi.spyOn(tab.api, 'logPage').mockImplementation(async (_id, skip, limit, _q, cursor) => {
    calls.push({ skip, limit, cursor: cursor ?? null });
    if (serve) return serve(skip, limit, cursor ?? null);
    if (cursor) {
      // Como Rust: recorre desde la frontera (aquí lineal: el primero de la frontera hacia atrás).
      const start = history.findIndex((c) => c.sha === cursor[0]);
      return history.slice(start, start + limit);
    }
    return history.slice(skip, skip + limit);
  });
  vi.spyOn(tab.api, 'diff').mockResolvedValue([]);
  return { tab, calls, h: new HistoryController(tab, page) };
}

describe('HistoryController: paginación por cursor', () => {
  it('la primera página va sin cursor y las siguientes continúan desde la frontera', async () => {
    const { h, calls } = await controllerWith(linear(25), 10);
    await h.reload();
    await h.loadMore();
    await h.loadMore();
    expect(h.commits.map((c) => c.sha)).toEqual(linear(25).map((c) => c.sha));
    expect(calls[0]).toMatchObject({ skip: 0, cursor: null });
    // Lineal: la frontera es el padre del último commit cargado.
    expect(calls[1]!.cursor).toEqual(['n10']);
    expect(calls[2]!.cursor).toEqual(['n20']);
    expect(h.hasMore).toBe(false);
  });

  it('con merges la frontera lleva todas las ramas pendientes', async () => {
    // m(x, y) → x → r ; y → r
    const history = [commit('m', ['x', 'y']), commit('x', ['r']), commit('y', ['r']), commit('r', [])];
    const { h, calls } = await controllerWith(history, 2, (_s, _l, cursor) => (cursor ? history.slice(history.findIndex((c) => c.sha === cursor[0])) : history.slice(0, 2)));
    await h.reload();
    await h.loadMore();
    expect(calls[1]!.cursor).toEqual(['y', 'r']);
  });

  it('no repite commits si el cursor devuelve alguno ya cargado (fechas iguales) y sigue', async () => {
    const history = linear(8);
    const { h } = await controllerWith(history, 3, (skip, limit, cursor) => {
      if (cursor) return [history[2]!, ...history.slice(3, 3 + limit - 1)]; // repite n2
      return history.slice(skip, skip + limit);
    });
    await h.reload();
    await h.loadMore();
    expect(h.commits.map((c) => c.sha)).toEqual(['n0', 'n1', 'n2', 'n3', 'n4']);
    expect(new Set(h.commits.map((c) => c.sha)).size).toBe(h.commits.length);
    expect(h.hasMore).toBe(true);
  });

  it('una página de puros repetidos se pide por skip para no quedarse atascado', async () => {
    const history = linear(9);
    const { h, calls } = await controllerWith(history, 3, (skip, limit, cursor) => (cursor ? history.slice(0, 3) : history.slice(skip, skip + limit)));
    await h.reload();
    await h.loadMore();
    expect(h.commits.map((c) => c.sha)).toEqual(['n0', 'n1', 'n2', 'n3', 'n4', 'n5']);
    expect(calls[1]!.cursor).not.toBeNull();
    expect(calls[2]).toMatchObject({ skip: 3, cursor: null });
  });

  it('con búsqueda se pagina por skip y nunca se manda cursor', async () => {
    const { h, calls } = await controllerWith(linear(30), 10);
    h.query = 'n';
    await h.reload();
    await h.loadMore();
    expect(calls.every((c) => c.cursor === null)).toBe(true);
    expect(calls[1]!.skip).toBe(10);
  });

  it('un historial cuyos commits no traen padres (mock) sigue paginando por skip', async () => {
    const flat = Array.from({ length: 12 }, (_, i) => commit(`f${i}`, []));
    const { h, calls } = await controllerWith(flat, 5);
    await h.reload();
    await h.loadMore();
    await h.loadMore();
    expect(h.commits).toHaveLength(12);
    expect(calls.slice(1).every((c) => c.cursor === null && c.skip > 0)).toBe(true);
  });

  it('recargar descarta la frontera anterior', async () => {
    const { h, calls } = await controllerWith(linear(20), 5);
    await h.reload();
    await h.loadMore();
    await h.reload();
    await h.loadMore();
    expect(calls[2]).toMatchObject({ skip: 0, cursor: null });
    expect(calls[3]!.cursor).toEqual(['n5']);
  });

  it('la API simulada acepta el cursor y sigue siendo compatible con skip', async () => {
    const api = mockApi();
    const state = await api.appState();
    const id = state.tabs[0]!.tabId;
    const a = await api.logPage(id, 2, 2, null);
    const b = await api.logPage(id, 2, 2, null, ['x'.repeat(40)]);
    expect(b).toEqual(a);
  });
});
