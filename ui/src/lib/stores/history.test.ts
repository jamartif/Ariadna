import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { TabState } from './tab.svelte';
import { loadedTab as loadAriadna, mockApi, resetGlobals, waitFor } from '../../test/helpers';
import { HistoryController } from './history.svelte';

beforeEach(resetGlobals);
afterEach(resetGlobals);

/** Pestaña del repo simulado con 1 500 commits (se abre desde recientes). */
async function loadedTab(id: string) {
  if (id !== 'tab-gigante') return loadAriadna(id);
  const api = mockApi();
  const res = await api.openRepository('/home/javie/proyectos/monorepo-gigante');
  const tab = new TabState(api, res.tab);
  await tab.attach();
  await tab.load();
  return { api, tab };
}

describe('HistoryController', () => {
  it('carga la primera página, selecciona el primer commit y trae sus archivos', async () => {
    const { tab } = await loadedTab('tab-gigante');
    const h = new HistoryController(tab, 50);
    await h.reload();
    expect(h.commits).toHaveLength(50);
    expect(h.hasMore).toBe(true);
    expect(h.rows).toHaveLength(50);
    expect(h.selected?.sha).toBe(h.commits[0]?.sha);
    expect(h.files.length).toBeGreaterThan(0);
    expect(h.file).toBe(h.files[0]);
  });

  it('pagina sin duplicados ni huecos y el grafo crece con ella', async () => {
    const { tab } = await loadedTab('tab-gigante');
    const h = new HistoryController(tab, 100);
    await h.reload();
    await h.loadMore();
    await h.loadMore();
    expect(h.commits).toHaveLength(300);
    expect(new Set(h.commits.map((c) => c.sha)).size).toBe(300);
    expect(h.rows).toHaveLength(300);
    expect(h.graphWidth).toBeGreaterThan(1);
  });

  it('llega al final: hasMore pasa a falso y no pide más', async () => {
    const { tab, api } = await loadedTab('tab-ariadna');
    const h = new HistoryController(tab, 100);
    await h.reload();
    expect(h.hasMore).toBe(false);
    const calls: number[] = [];
    const orig = api.logPage.bind(api);
    api.logPage = async (...a) => (calls.push(a[1]), orig(...a));
    await h.loadMore();
    expect(calls).toEqual([]);
  });

  it('buscar filtra en todo el historial, quita el grafo y limpia al vaciar la búsqueda', async () => {
    const { tab } = await loadedTab('tab-gigante');
    const h = new HistoryController(tab, 40);
    await h.reload();
    h.query = 'tarea-7';
    await h.reload();
    expect(h.commits.length).toBeGreaterThan(0);
    expect(h.commits.every((c) => c.subject.includes('tarea-7'))).toBe(true);
    expect(h.rows).toHaveLength(0);
    h.query = '';
    await h.reload();
    expect(h.rows).toHaveLength(40);
  });

  it('una respuesta vieja no pisa a la nueva', async () => {
    const { tab, api } = await loadedTab('tab-gigante');
    const h = new HistoryController(tab, 20);
    const orig = api.logPage.bind(api);
    let first = true;
    api.logPage = async (...a) => {
      const slow = first;
      first = false;
      const res = await orig(...a);
      if (slow) await new Promise((r) => setTimeout(r, 30));
      return res;
    };
    h.query = 'tarea-1';
    const a = h.reload();
    h.query = 'tarea-2';
    const b = h.reload();
    await Promise.all([a, b]);
    expect(h.commits.every((c) => c.subject.includes('tarea-2'))).toBe(true);
  });

  it('mover la selección con el teclado y cambiar de archivo', async () => {
    const { tab } = await loadedTab('tab-gigante');
    const h = new HistoryController(tab, 30);
    await h.reload();
    await h.move(1);
    expect(h.selected?.sha).toBe(h.commits[1]?.sha);
    await h.move(-5);
    expect(h.selected?.sha).toBe(h.commits[0]?.sha);
    await waitFor(() => expect(h.files.length).toBeGreaterThan(0));
    h.selectFile(h.files.length - 1);
    expect(h.fileIndex).toBe(h.files.length - 1);
    h.selectFile(99);
    expect(h.fileIndex).toBe(h.files.length - 1);
  });

  it('un error de la API queda en `error` sin romper la lista', async () => {
    const { tab, api } = await loadedTab('tab-ariadna');
    const h = new HistoryController(tab, 20);
    api.logPage = async () => {
      throw { code: 'COMMAND_FAILED', message: 'git falló' };
    };
    await h.reload();
    expect(h.error?.message).toBe('git falló');
    expect(h.loading).toBe(false);
  });
});
