import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { Snapshot } from '$lib/api/types';
import { SOFT_TAB_LIMIT } from './app.svelte';
import { toasts } from './toasts.svelte';
import { loadedTab, readyApp, resetGlobals, waitFor } from '../../test/helpers';

beforeEach(resetGlobals);
afterEach(resetGlobals);

function withRevision(s: Snapshot, revision: number): Snapshot {
  return { ...s, revision };
}

describe('TabState: revisiones', () => {
  it('descarta snapshots con revisión menor o igual', async () => {
    const { tab } = await loadedTab();
    const base = tab.snapshot!;
    const r = base.revision;
    const newer = withRevision(base, r + 5);
    expect(tab.applySnapshot(newer)).toBe(true);
    expect(tab.revision).toBe(r + 5);

    const older = withRevision({ ...base, status: { ...base.status, entries: [] } }, r + 2);
    expect(tab.applySnapshot(older)).toBe(false);
    expect(tab.revision).toBe(r + 5);
    expect(tab.snapshot!.status.entries.length).toBeGreaterThan(0); // no se pisó con el viejo
    expect(tab.applySnapshot(withRevision(base, r + 5))).toBe(false); // igual también
    expect(tab.discardedSnapshots).toBe(2);
  });

  it('un snapshot viejo que llega por el evento tampoco pisa el estado', async () => {
    const { tab, api } = await loadedTab();
    const before = tab.revision;
    await api.stage(tab.id, ['docs/ipc.md']);
    await waitFor(() => expect(tab.revision).toBeGreaterThan(before));
    const current = tab.snapshot!;
    expect(tab.applySnapshot(withRevision(current, before))).toBe(false);
    expect(tab.snapshot).toBe(current);
  });

  it('ignora eventos de otras pestañas', async () => {
    const { tab, api } = await loadedTab();
    const rev = tab.revision;
    const other = (await api.getSnapshot('tab-legacy'));
    await api.stage('tab-legacy', ['README.md']);
    await new Promise((r) => setTimeout(r, 30));
    expect(tab.revision).toBe(rev);
    expect(other.revision).toBeGreaterThan(0);
  });

  it('la selección sigue al archivo cuando cambia de sección', async () => {
    const { tab } = await loadedTab();
    const entry = tab.sections.unstaged.find((e) => e.path === 'docs/ipc.md')!;
    tab.select('unstaged', entry);
    await tab.stage('unstaged', [entry]);
    await waitFor(() => expect(tab.selected?.section).toBe('staged'));
    expect(tab.selected?.path).toBe('docs/ipc.md');
  });

  it('si el archivo seleccionado desaparece se elige el vecino', async () => {
    const { tab } = await loadedTab();
    const entry = tab.sections.unstaged.find((e) => e.path === 'docs/ipc.md')!;
    tab.select('unstaged', entry);
    await tab.discard('unstaged', [entry]);
    await waitFor(() => expect(tab.selected?.path).not.toBe('docs/ipc.md'));
    expect(tab.selected).not.toBeNull();
  });

  it('registra operaciones en curso desde session-event', async () => {
    const { tab, api } = await loadedTab();
    let seen = '';
    const un = await api.listen('session-event', () => {
      seen = tab.operations[0]?.label ?? seen;
    });
    await api.stage(tab.id, ['docs/ipc.md']);
    un();
    expect(seen).toContain('Preparando');
    await waitFor(() => expect(tab.operations).toHaveLength(0));
  });

  it('un repo desaparecido da sessionError REPO_MISSING', async () => {
    const { api } = await loadedTab();
    const { TabState } = await import('./tab.svelte');
    const info = (await api.appState()).tabs.find((t) => t.missing)!;
    const missing = new TabState(api, info);
    await missing.load();
    expect(missing.sessionError?.code).toBe('REPO_MISSING');
  });
});

describe('AppStore: pestañas', () => {
  it('abre, enfoca el ya abierto y cierra', async () => {
    const { app } = await readyApp();
    expect(app.entries.map((e) => e.id)).toEqual(['tab-ariadna', 'tab-legacy', 'tab-antiguo']);
    expect(app.activeId).toBe('tab-ariadna');
    const res = await app.openPath('/home/javie/proyectos/legacy-api');
    expect(res.ok).toBe(true);
    expect(app.activeId).toBe('tab-legacy');
    expect(app.entries).toHaveLength(3);
    expect(await app.close('tab-legacy')).toBe(true);
    expect(app.entries.map((e) => e.id)).not.toContain('tab-legacy');
    expect(app.activeId).not.toBe('tab-legacy');
  });

  it('al cerrar la última pestaña queda una de bienvenida', async () => {
    const { app } = await readyApp({ scenario: 'empty' });
    expect(app.entries).toHaveLength(1);
    expect(app.entries[0]!.kind).toBe('new');
    await app.close(app.entries[0]!.id);
    expect(app.entries).toHaveLength(1);
    expect(app.entries[0]!.kind).toBe('new');
  });

  it('newTab no duplica la pestaña nueva y avisa al pasar el tope blando', async () => {
    const { app } = await readyApp();
    app.newTab();
    app.newTab();
    expect(app.entries.filter((e) => e.kind === 'new')).toHaveLength(1);
    // Llena hasta el tope con repos nuevos
    for (let i = 0; i < SOFT_TAB_LIMIT; i++) await app.openPath(`/home/javie/repos/r${i}`);
    expect(app.entries.length).toBeGreaterThanOrEqual(SOFT_TAB_LIMIT);
    toasts.clear();
    const draft = app.entries.find((e) => e.kind === 'new');
    if (draft) await app.close(draft.id);
    app.newTab();
    expect(toasts.items.some((t) => t.kind === 'warning' && t.title === 'Muchas pestañas abiertas')).toBe(true);
  });

  it('reordena y avisa a Rust solo con los ids reales', async () => {
    const { app, api } = await readyApp();
    app.newTab();
    const spy = vi.spyOn(api, 'reorderTabs');
    app.moveTab('tab-ariadna', 1);
    expect(app.entries.map((e) => e.id).slice(0, 2)).toEqual(['tab-legacy', 'tab-ariadna']);
    expect(spy).toHaveBeenCalledWith(['tab-legacy', 'tab-ariadna', 'tab-antiguo']);
    app.moveTab('tab-legacy', -5); // ya es la primera
    expect(app.entries[0]!.id).toBe('tab-legacy');
  });

  it('Ctrl+N y siguiente/anterior circulan', async () => {
    const { app } = await readyApp();
    app.gotoIndex(2);
    expect(app.activeId).toBe('tab-legacy');
    app.step(1);
    expect(app.activeId).toBe('tab-antiguo');
    app.step(1);
    expect(app.activeId).toBe('tab-ariadna');
    app.step(-1);
    expect(app.activeId).toBe('tab-antiguo');
    app.gotoIndex(9); // no existe: no cambia
    expect(app.activeId).toBe('tab-antiguo');
  });

  it('tab-updated actualiza los indicadores de la pestaña', async () => {
    const { app, api } = await readyApp();
    const tab = app.tabState('tab-ariadna')!;
    await waitFor(() => expect(tab.snapshot).not.toBeNull());
    const before = (app.entries[0] as { info: { changeCount: number } }).info.changeCount;
    await api.discard('tab-ariadna', ['docs/ipc.md']);
    await waitFor(() => expect((app.entries[0] as { info: { changeCount: number } }).info.changeCount).toBe(before - 1));
  });
});
