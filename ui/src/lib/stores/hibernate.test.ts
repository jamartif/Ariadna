import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { AppStore, HIBERNATE_AFTER_MS } from './app.svelte';
import { historyOf } from './history.svelte';
import { mockApi, resetGlobals, waitFor } from '../../test/helpers';

beforeEach(resetGlobals);
afterEach(resetGlobals);

async function setup() {
  let now = 1_000_000;
  const api = mockApi();
  const app = new AppStore(api, () => now);
  await app.init();
  const tick = (ms: number) => (now += ms);
  /** Carga (y deja cargada) la pestaña indicada. */
  const load = async (id: string) => {
    const tab = app.tabState(id)!;
    await waitFor(() => expect(tab.snapshot).not.toBeNull());
    return tab;
  };
  return { api, app, tick, load };
}

describe('hibernación de pestañas', () => {
  it('las pestañas ocultas liberan snapshot e historial tras 10 minutos; la activa no', async () => {
    const { app, tick, load } = await setup();
    const a = await load('tab-ariadna');
    app.activate('tab-legacy');
    const hist = historyOf(a);
    await hist.reload();
    expect(hist.commits.length).toBeGreaterThan(0);

    // Primera pasada: arranca el reloj. Antes del plazo no pasa nada.
    app.hibernateIdle();
    tick(HIBERNATE_AFTER_MS - 1000);
    expect(app.hibernateIdle()).toEqual([]);
    expect(a.hibernated).toBe(false);

    tick(2000);
    expect(app.hibernateIdle()).toContain('tab-ariadna');
    expect(a.hibernated).toBe(true);
    expect(a.snapshot).toBeNull();
    expect(hist.commits).toEqual([]);
    expect(hist.rows).toEqual([]);
    expect(hist.selected).toBeNull();
    expect(hist.files).toEqual([]);
  });

  it('nunca hiberna la pestaña activa', async () => {
    const { app, tick, load } = await setup();
    const a = await load('tab-ariadna');
    app.hibernateIdle();
    tick(HIBERNATE_AFTER_MS * 3);
    expect(app.hibernateIdle()).not.toContain('tab-ariadna');
    expect(a.hibernated).toBe(false);
    expect(a.snapshot).not.toBeNull();
  });

  it('no suelta lo que no debe: integración en curso, conflictos, operaciones o commit', async () => {
    const { app, tick, load } = await setup();
    const a = await load('tab-ariadna');
    const c = await load('tab-legacy');
    app.activate('tab-antiguo');
    app.hibernateIdle();
    tick(HIBERNATE_AFTER_MS + 1);
    // Con conflictos pendientes la pestaña conserva su estado (hay ediciones sin aplicar).
    expect(c.info.conflictCount).toBeGreaterThan(0);
    expect(app.hibernateIdle()).toEqual(['tab-ariadna']);
    expect(c.hibernated).toBe(false);
    expect(c.snapshot).not.toBeNull();
    // Con una operación en curso tampoco (se despierta la otra para probarlo).
    a.wake();
    await waitFor(() => expect(a.snapshot).not.toBeNull());
    a.operations = [{ id: 1, label: 'fetch' }];
    expect(a.hibernate()).toBe(false);
    a.operations = [];
    a.committing = true;
    expect(a.hibernate()).toBe(false);
    a.committing = false;
    expect(a.hibernate()).toBe(true);
  });

  it('al activarla de nuevo se rehidrata y conserva el borrador del commit', async () => {
    const { app, tick, load } = await setup();
    const a = await load('tab-ariadna');
    a.setSummary('Borrador en curso');
    app.activate('tab-legacy');
    app.hibernateIdle();
    tick(HIBERNATE_AFTER_MS + 1);
    app.hibernateIdle();
    expect(a.hibernated).toBe(true);

    app.activate('tab-ariadna');
    expect(a.hibernated).toBe(false);
    await waitFor(() => expect(a.snapshot).not.toBeNull());
    expect(a.summary).toBe('Borrador en curso');
    // El reloj de la que acaba de ocultarse empieza de cero.
    expect(app.hibernateIdle()).toEqual([]);
  });

  it('una pestaña hibernada ignora los snapshots que llegan mientras duerme', async () => {
    const { api, app, tick, load } = await setup();
    const a = await load('tab-ariadna');
    app.activate('tab-legacy');
    app.hibernateIdle();
    tick(HIBERNATE_AFTER_MS + 1);
    app.hibernateIdle();
    expect(a.hibernated).toBe(true);
    await api.stage('tab-ariadna', ['docs/ipc.md']);
    await new Promise((r) => setTimeout(r, 30));
    expect(a.snapshot).toBeNull();
    app.activate('tab-ariadna');
    await waitFor(() => expect(a.snapshot).not.toBeNull());
    expect(a.sections.staged.some((e) => String(e.path) === 'docs/ipc.md')).toBe(true);
  });
});
