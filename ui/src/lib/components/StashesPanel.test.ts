import { cleanup, render, screen, waitFor, within } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { stashOf } from '$lib/stores/stash.svelte';
import { toasts } from '$lib/stores/toasts.svelte';
import { buildActions } from '$lib/stores/commands';
import { AppStore } from '$lib/stores/app.svelte';
import { check, stashApplyResult, stashEntry } from '$lib/api/validate';
import { loadedTab, mockApi, resetGlobals } from '../../test/helpers';
import { expectNoSeriousViolations } from '../../test/axe';
import ChangesPanel from './ChangesPanel.svelte';
import Sidebar from './Sidebar.svelte';
import StashDetail from './StashDetail.svelte';
import StashesPanel from './StashesPanel.svelte';

beforeEach(resetGlobals);
afterEach(() => {
  cleanup();
  resetGlobals();
});

async function withStash(opts: Parameters<typeof loadedTab>[1] = {}, message = 'mi trabajo a medias') {
  const ctx = await loadedTab('tab-ariadna', opts);
  const s = stashOf(ctx.tab);
  const entry = await ctx.api.stashPush(ctx.tab.id, { message, includeUntracked: true });
  await s.load();
  return { ...ctx, s, entry };
}

describe('Stashes: panel y detalle', () => {
  it('sin stashes muestra el estado vacío y el botón de guardar', async () => {
    const { tab } = await loadedTab();
    render(StashesPanel, { props: { tab } });
    expect(await screen.findByText('No hay stashes')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Guardar cambios en stash' })).toBeInTheDocument();
  });

  it('lista los stashes con rama y mensaje, y al elegir uno muestra sus archivos y su diff', async () => {
    const { tab, entry } = await withStash();
    render(StashesPanel, { props: { tab } });
    render(StashDetail, { props: { tab } });
    const item = await screen.findByRole('button', { name: /mi trabajo a medias/ });
    expect(item).toHaveTextContent(`en ${entry.branch}`);
    expect(item).toHaveTextContent('stash@{0}');
    // Se selecciona el primero solo y se carga su contenido.
    await waitFor(() => expect(screen.getByRole('heading', { level: 2, name: 'mi trabajo a medias' })).toBeInTheDocument());
    const files = await screen.findByRole('list', { name: 'Archivos del stash' });
    expect(within(files).getAllByRole('button').length).toBeGreaterThan(1);
    expect(document.querySelectorAll('.line').length).toBeGreaterThan(0);
  });

  it('Aplicar conserva el stash; Sacar lo borra de la lista', async () => {
    const { tab, s, api, entry } = await withStash();
    render(StashesPanel, { props: { tab } });
    render(StashDetail, { props: { tab } });
    await screen.findByRole('button', { name: 'Aplicar' });
    await userEvent.click(screen.getByRole('button', { name: 'Aplicar' }));
    await waitFor(() => expect(toasts.items.some((x) => x.title === 'Stash aplicado. Se conserva en la lista.')).toBe(true));
    expect(s.list).toHaveLength(1);
    // Hay que dejar el árbol limpio para poder sacarlo: guardamos de nuevo lo aplicado.
    await api.stashPush(tab.id, { message: 'otro', includeUntracked: true });
    await s.load();
    const pop = s.list.find((x) => x.oid === entry.oid)!;
    await s.select(pop.oid);
    await userEvent.click(await screen.findByRole('button', { name: 'Sacar' }));
    await waitFor(() => expect(s.list.some((x) => x.oid === entry.oid)).toBe(false));
    expect(toasts.items.some((x) => x.title === 'Stash aplicado y borrado de la lista.')).toBe(true);
  });

  it('Borrar pide confirmación nativa: si se cancela no se borra', async () => {
    const { tab, s } = await withStash({ confirm: () => false });
    render(StashesPanel, { props: { tab } });
    render(StashDetail, { props: { tab } });
    await userEvent.click(await screen.findByRole('button', { name: 'Borrar' }));
    await waitFor(() => expect(s.busy).toBeNull());
    expect(s.list).toHaveLength(1);
    expect(toasts.items.some((x) => x.title === 'Stash borrado.')).toBe(false);
  });

  it('Borrar con confirmación lo quita de la lista', async () => {
    const { tab, s } = await withStash({ confirm: () => true });
    render(StashesPanel, { props: { tab } });
    render(StashDetail, { props: { tab } });
    await userEvent.click(await screen.findByRole('button', { name: 'Borrar' }));
    await waitFor(() => expect(s.list).toHaveLength(0));
    expect(await screen.findByText('No hay stashes')).toBeInTheDocument();
    expect(screen.getByText('Elige un stash')).toBeInTheDocument();
  });

  it('si aplicar deja conflictos: aviso, conflictos en el repo y se abre el panel de Conflictos', async () => {
    const { tab, s, entry } = await withStash({ scenario: 'stash-conflict' });
    tab.panel = 'stashes';
    const ok = await s.pop(entry.oid);
    expect(ok).toBe(true);
    // El stash se conserva y el snapshot ya trae el archivo en conflicto.
    expect(s.list).toHaveLength(1);
    expect(tab.snapshot?.conflicts.length).toBeGreaterThan(0);
    expect(tab.panel).toBe('conflicts');
    const warn = toasts.items.find((x) => x.title === 'El stash ha dejado conflictos');
    expect(warn).toBeDefined();
    expect(warn!.message).toContain('Resuélvelo en Conflictos');
  });

  it('un stash que ya no existe (STASH_NOT_FOUND) avisa y relee la lista', async () => {
    const { tab, s, api } = await withStash();
    const first = s.list[0]!.oid;
    await api.stashDrop(tab.id, first);
    await s.apply(first);
    await waitFor(() => expect(s.list).toHaveLength(0));
    expect(toasts.items.some((x) => x.title === 'No se pudo aplicar el stash')).toBe(true);
  });
});

describe('Stashes: guardar desde Cambios', () => {
  it('el botón abre el formulario y guarda con mensaje y archivos sin seguimiento', async () => {
    const { tab } = await loadedTab();
    const s = stashOf(tab);
    const before = tab.info.changeCount;
    render(ChangesPanel, { props: { tab } });
    await userEvent.click(screen.getByRole('button', { name: 'Guardar cambios en stash' }));
    const form = await screen.findByRole('form', { name: 'Guardar cambios en un stash' });
    const msg = within(form).getByLabelText('Mensaje (opcional)');
    expect(msg).toHaveFocus();
    await userEvent.type(msg, 'pausa para la reunión');
    await userEvent.click(within(form).getByLabelText('Incluir archivos sin seguimiento'));
    await userEvent.click(within(form).getByRole('button', { name: 'Guardar' }));
    await waitFor(() => expect(s.list).toHaveLength(1));
    expect(s.list[0]!.message).toBe('pausa para la reunión');
    expect(s.pushOpen).toBe(false);
    expect(toasts.items.some((x) => x.title === 'Cambios guardados en el stash')).toBe(true);
    await waitFor(() => expect(tab.info.changeCount).toBeLessThan(before));
  });

  it('«Solo lo preparado» guarda únicamente lo preparado', async () => {
    const { tab, api } = await loadedTab();
    const s = stashOf(tab);
    const stagedBefore = tab.stagedCount;
    expect(stagedBefore).toBeGreaterThan(0);
    render(ChangesPanel, { props: { tab } });
    await userEvent.click(screen.getByRole('button', { name: 'Guardar cambios en stash' }));
    const form = await screen.findByRole('form', { name: 'Guardar cambios en un stash' });
    await userEvent.click(within(form).getByLabelText('Solo lo preparado'));
    expect(within(form).getByLabelText('Incluir archivos sin seguimiento')).toBeDisabled();
    await userEvent.click(within(form).getByRole('button', { name: 'Guardar' }));
    await waitFor(() => expect(s.list).toHaveLength(1));
    const snap = await api.getSnapshot(tab.id);
    expect(snap.status.entries.some((e) => e.indexStatus !== '.' && e.indexStatus !== '?' && !e.isIgnored)).toBe(false);
    expect(snap.status.entries.some((e) => e.workTreeStatus !== '.')).toBe(true);
  });

  it('Esc cierra el formulario y «Solo lo preparado» se desactiva sin nada preparado', async () => {
    const { tab } = await loadedTab();
    const s = stashOf(tab);
    await tab.unstageAll();
    await waitFor(() => expect(tab.stagedCount).toBe(0));
    render(ChangesPanel, { props: { tab } });
    await userEvent.click(screen.getByRole('button', { name: 'Guardar cambios en stash' }));
    const form = await screen.findByRole('form', { name: 'Guardar cambios en un stash' });
    expect(within(form).getByLabelText('Solo lo preparado')).toBeDisabled();
    await userEvent.keyboard('{Escape}');
    await waitFor(() => expect(s.pushOpen).toBe(false));
  });

  it('sin nada que guardar avisa con un mensaje claro', async () => {
    const { tab } = await loadedTab();
    const s = stashOf(tab);
    expect(await s.push({ includeUntracked: true })).toBe(true);
    // Ya no queda nada (ni sin seguimiento): git no crea el stash.
    await waitFor(() => expect(tab.info.changeCount).toBe(0));
    expect(await s.push({ includeUntracked: true })).toBe(false);
    expect(toasts.items.some((x) => x.title === 'No hay nada que guardar')).toBe(true);
    expect(s.list).toHaveLength(1);
  });
});

describe('Stashes: barra lateral, paleta y contrato', () => {
  it('la barra lateral lista los stashes y los abre en su panel', async () => {
    const { tab, s } = await withStash();
    render(Sidebar, { props: { tab, snapshot: tab.snapshot, loading: false } });
    const btn = await screen.findByRole('button', { name: /Abrir el stash «mi trabajo a medias»/ });
    await userEvent.click(btn);
    expect(tab.panel).toBe('stashes');
    await waitFor(() => expect(s.selectedOid).toBe(s.list[0]!.oid));
  });

  it('la paleta ofrece guardar, mostrar y sacar el último stash', async () => {
    const api = mockApi();
    const app = new AppStore(api);
    await app.init();
    const tab = app.activeTab!;
    const s = stashOf(tab);
    let ids = buildActions(app).map((a) => a.id);
    expect(ids).toContain('stash.push');
    expect(ids).toContain('stash.open');
    expect(ids).not.toContain('stash.popLatest');
    await s.push({ message: 'último', includeUntracked: true });
    ids = buildActions(app).map((a) => a.id);
    expect(ids).toContain('stash.popLatest');
    const open = buildActions(app).find((a) => a.id === 'stash.open')!;
    expect(open.shortcut).toMatch(/H$/);
    buildActions(app).find((a) => a.id === 'stash.push')!.run();
    expect(tab.panel).toBe('changes');
    expect(s.pushOpen).toBe(true);
  });

  it('las formas del mock siguen los validadores del contrato', async () => {
    const { tab, api, s } = await withStash();
    expect(check(stashEntry, s.list[0])).toEqual([]);
    const res = await api.stashApply(tab.id, s.list[0]!.oid).catch(() => null);
    if (res) expect(check(stashApplyResult, res)).toEqual([]);
    const detached = await api.stashList(tab.id);
    expect(detached.every((e) => check(stashEntry, e).length === 0)).toBe(true);
  });
});

describe('axe: Stashes', () => {
  it('panel, detalle y formulario', async () => {
    const { tab, s } = await withStash();
    s.openPush();
    render(StashesPanel, { props: { tab } });
    render(StashDetail, { props: { tab } });
    await screen.findByRole('heading', { level: 2, name: 'mi trabajo a medias' });
    await screen.findByRole('list', { name: 'Archivos del stash' });
    await expectNoSeriousViolations();
  });

  it('estado vacío', async () => {
    const { tab } = await loadedTab();
    render(StashesPanel, { props: { tab } });
    render(StashDetail, { props: { tab } });
    await screen.findByText('No hay stashes');
    await expectNoSeriousViolations();
  });
});
