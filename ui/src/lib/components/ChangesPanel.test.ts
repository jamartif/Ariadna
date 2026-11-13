import { render, screen, waitFor, within } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { toasts } from '$lib/stores/toasts.svelte';
import { TabState } from '$lib/stores/tab.svelte';
import { loadedTab, mockApi, resetGlobals } from '../../test/helpers';
import ChangesPanel from './ChangesPanel.svelte';

beforeEach(resetGlobals);
afterEach(resetGlobals);

const sectionHeaders = () => screen.getAllByRole('row').filter((r) => r.classList.contains('header')).map((r) => r.textContent?.replace(/\s+/g, ' ').trim());

describe('ChangesPanel con mockApi', () => {
  it('agrupa en Preparados / Sin preparar / Sin seguimiento / Conflictos y oculta ignorados', async () => {
    const { tab } = await loadedTab();
    render(ChangesPanel, { props: { tab } });
    const headers = sectionHeaders();
    expect(headers.some((h) => h?.startsWith('Preparados'))).toBe(true);
    expect(headers.some((h) => h?.startsWith('Sin preparar'))).toBe(true);
    expect(headers.some((h) => h?.startsWith('Sin seguimiento'))).toBe(true);
    expect(headers.some((h) => h?.startsWith('Ignorados'))).toBe(false);
    expect(screen.queryByText('.env.local')).not.toBeInTheDocument();
  });

  it('el interruptor muestra los ignorados', async () => {
    const { tab } = await loadedTab();
    render(ChangesPanel, { props: { tab } });
    await userEvent.click(screen.getByRole('switch', { name: /Ignorados/ }));
    expect(await screen.findByText('.env.local')).toBeInTheDocument();
    expect(sectionHeaders().some((h) => h?.startsWith('Ignorados'))).toBe(true);
  });

  it('preparar un archivo lo mueve a Preparados con una revisión nueva', async () => {
    const { tab } = await loadedTab();
    render(ChangesPanel, { props: { tab } });
    const before = tab.revision;
    expect(tab.sections.staged.some((e) => e.path === 'docs/ipc.md')).toBe(false);
    await userEvent.click(screen.getByRole('checkbox', { name: 'Preparar docs/ipc.md' }));
    await waitFor(() => expect(tab.sections.staged.some((e) => e.path === 'docs/ipc.md')).toBe(true));
    expect(tab.revision).toBeGreaterThan(before);
    expect(await screen.findByRole('checkbox', { name: 'Quitar de preparados docs/ipc.md' })).toBeChecked();
    expect(tab.sections.unstaged.some((e) => e.path === 'docs/ipc.md')).toBe(false);
  });

  it('quitar de preparados devuelve el archivo a Sin preparar', async () => {
    const { tab } = await loadedTab();
    render(ChangesPanel, { props: { tab } });
    await userEvent.click(screen.getByRole('checkbox', { name: 'Quitar de preparados docs/README.md' }));
    await waitFor(() => expect(tab.sections.staged.some((e) => e.path === 'docs/README.md')).toBe(false));
    expect(tab.sections.unstaged.some((e) => e.path === 'docs/README.md')).toBe(true);
  });

  it('«Quitar todo» vacía Preparados', async () => {
    const { tab } = await loadedTab();
    render(ChangesPanel, { props: { tab } });
    expect(tab.stagedCount).toBeGreaterThan(0);
    await userEvent.click(screen.getByRole('button', { name: 'Quitar todo' }));
    await waitFor(() => expect(tab.stagedCount).toBe(0));
    expect(await screen.findAllByRole('button', { name: 'Preparar todo' })).not.toHaveLength(0);
  });

  it('«Preparar todo» de Sin preparar prepara los modificados', async () => {
    const { tab } = await loadedTab('tab-legacy');
    render(ChangesPanel, { props: { tab } });
    expect(tab.sections.unstaged.length).toBeGreaterThan(0);
    const header = screen.getAllByRole('row').find((r) => r.textContent?.trim().startsWith('Sin preparar')) as HTMLElement;
    await userEvent.click(within(header).getByRole('button', { name: 'Preparar todo' }));
    await waitFor(() => expect(tab.sections.unstaged).toHaveLength(0));
  });

  it('un error de operación se muestra como toast con detalle y no cambia el estado', async () => {
    const { tab } = await loadedTab();
    render(ChangesPanel, { props: { tab } });
    await userEvent.click(screen.getByRole('checkbox', { name: 'Preparar docs/error-demo.md' }));
    await waitFor(() => expect(toasts.items).toHaveLength(1));
    const toast = toasts.items[0]!;
    expect(toast.kind).toBe('error');
    expect(toast.title).toBe('No se pudo preparar');
    expect(toast.detail).toContain('index.lock');
    expect(tab.sections.untracked.some((e) => e.path === 'docs/error-demo.md')).toBe(true);
  });

  it('descartar llama al comando (la confirmación es nativa) y respeta la cancelación', async () => {
    const api = mockApi({ confirm: () => false });
    const info = (await api.appState()).tabs[0]!;
    const tab = new TabState(api, info);
    await tab.attach();
    await tab.load();
    render(ChangesPanel, { props: { tab } });
    await userEvent.click(screen.getByRole('button', { name: 'Descartar cambios de docs/ipc.md' }));
    await new Promise((r) => setTimeout(r, 20));
    expect(tab.sections.unstaged.some((e) => e.path === 'docs/ipc.md')).toBe(true);
  });

  it('descartar confirmado elimina el cambio', async () => {
    const { tab } = await loadedTab();
    render(ChangesPanel, { props: { tab } });
    await userEvent.click(screen.getByRole('button', { name: 'Descartar cambios de docs/ipc.md' }));
    await waitFor(() => expect(tab.sections.unstaged.some((e) => e.path === 'docs/ipc.md')).toBe(false));
  });

  it('teclado: flechas seleccionan, Espacio prepara', async () => {
    const { tab } = await loadedTab();
    render(ChangesPanel, { props: { tab } });
    const grid = screen.getByRole('grid');
    grid.focus();
    await userEvent.keyboard('{Home}{ArrowDown}');
    // Fila 0 = cabecera «Preparados»; fila 1 = primer archivo preparado.
    expect(tab.selected?.section).toBe('staged');
    const sel = tab.selected!;
    expect(grid.getAttribute('aria-activedescendant')).toMatch(/-r1$/);
    await userEvent.keyboard('{ArrowDown}');
    expect(tab.selected?.path).not.toBe(sel.path);
    await userEvent.keyboard('{ArrowUp} ');
    await waitFor(() => expect(tab.sections.staged.some((e) => e.path === sel.path)).toBe(false));
  });

  it('el filtro reduce la lista y muestra estado vacío sin coincidencias', async () => {
    const { tab } = await loadedTab();
    render(ChangesPanel, { props: { tab } });
    await userEvent.type(screen.getByRole('searchbox'), 'ipc');
    expect(screen.getByText('ipc.md')).toBeInTheDocument();
    expect(screen.queryByText('Makefile')).not.toBeInTheDocument();
    await userEvent.clear(screen.getByRole('searchbox'));
    await userEvent.type(screen.getByRole('searchbox'), 'zzzz-no-existe');
    expect(screen.getByText('Sin coincidencias')).toBeInTheDocument();
  });

  it('virtualiza: con 3 000 archivos solo se pintan unas decenas de filas', async () => {
    const api = mockApi();
    const { tab: info } = await api.openRepository('/home/javie/proyectos/monorepo-gigante');
    const tab = new TabState(api, info);
    await tab.attach();
    await tab.load();
    expect(tab.sections.unstaged).toHaveLength(3000);
    render(ChangesPanel, { props: { tab } });
    const grid = screen.getByRole('grid');
    expect(grid.getAttribute('aria-rowcount')).toBe('3001');
    const rendered = screen.getAllByRole('row').length;
    expect(rendered).toBeGreaterThan(5);
    expect(rendered).toBeLessThan(60);
  });

  it('estado vacío cuando el árbol está limpio', async () => {
    const { tab } = await loadedTab();
    tab.snapshot = { ...tab.snapshot!, status: { ...tab.snapshot!.status, entries: [] } };
    render(ChangesPanel, { props: { tab } });
    expect(screen.getByText('Todo en orden')).toBeInTheDocument();
    expect(screen.queryByRole('grid')).not.toBeInTheDocument();
  });

  it('muestra esqueleto mientras carga', async () => {
    const api = mockApi({ latency: 30 });
    const info = (await api.appState()).tabs[0]!;
    const tab = new TabState(api, info);
    render(ChangesPanel, { props: { tab } });
    expect(screen.getByLabelText('Cargando cambios')).toHaveAttribute('aria-busy', 'true');
    await tab.load();
    await waitFor(() => expect(screen.queryByLabelText('Cargando cambios')).not.toBeInTheDocument());
  });
});
