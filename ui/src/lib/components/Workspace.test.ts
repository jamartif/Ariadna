import { render, screen, waitFor, within } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { branchesOf } from '$lib/stores/branches.svelte';
import { runShortcut } from '$lib/shortcuts/registry';
import { loadedTab, resetGlobals } from '../../test/helpers';
import Workspace from './Workspace.svelte';

beforeEach(resetGlobals);
afterEach(() => {
  resetGlobals();
  vi.restoreAllMocks();
});

async function setup(opts = {}) {
  const ctx = await loadedTab('tab-ariadna', opts);
  render(Workspace, { props: { tab: ctx.tab, onclose: vi.fn(), onlocate: vi.fn() } });
  await screen.findByRole('tablist', { name: 'Secciones del repositorio' });
  return ctx;
}

const tabByName = (name: string | RegExp) => screen.getByRole('tab', { name });

describe('Espacio de trabajo: riel + vistas + detalle', () => {
  it('muestra el riel de ramas siempre a la izquierda y ya no duplica «Ramas» en el centro', async () => {
    await setup();
    expect(await screen.findByRole('navigation', { name: 'Árbol de ramas' })).toBeInTheDocument();
    expect(screen.queryByRole('tab', { name: /^Ramas/ })).toBeNull();
    const list = screen.getByRole('tablist', { name: 'Secciones del repositorio' });
    expect(within(list).getAllByRole('tab').map((t) => t.textContent?.trim().split(/\s+/)[0])).toEqual(
      expect.arrayContaining(['Cambios', 'Historial', 'Stashes'])
    );
  });

  it('navega entre vistas con las flechas y conserva el riel', async () => {
    await setup();
    const changes = tabByName(/^Cambios/);
    changes.focus();
    await userEvent.keyboard('{ArrowRight}');
    await waitFor(() => expect(tabByName(/^Historial/)).toHaveAttribute('aria-selected', 'true'));
    expect(screen.getByRole('navigation', { name: 'Árbol de ramas' })).toBeInTheDocument();
    await userEvent.keyboard('{ArrowLeft}');
    await waitFor(() => expect(tabByName(/^Cambios/)).toHaveAttribute('aria-selected', 'true'));
  });

  it('cambia la vista desde el control segmentado y muestra su detalle a la derecha', async () => {
    await setup();
    await userEvent.click(tabByName(/^Historial/));
    expect(await screen.findByRole('listbox', { name: 'Lista de commits' })).toBeInTheDocument();
    await userEvent.click(tabByName(/^Stashes/));
    expect((await screen.findAllByRole('region', { name: 'Stashes del repositorio' })).length).toBeGreaterThan(0);
    expect(tabByName(/^Stashes/)).toHaveAttribute('aria-selected', 'true');
  });

  it('el detalle de una rama sustituye el detalle de la vista y se cierra con «Volver»', async () => {
    const { tab } = await setup();
    const other = tab.snapshot!.branches.find((b) => !b.isHead)!;
    const row = [...document.querySelectorAll<HTMLElement>('[data-brail-name]')].find((b) => b.dataset.brailName === other.name)!;
    await userEvent.click(row);
    await userEvent.click(screen.getByRole('button', { name: /^Detalle de / }));
    expect(screen.getByText('Detalle de la rama')).toBeInTheDocument();
    expect(screen.getByRole('region', { name: 'Detalle de la rama' })).toBeInTheDocument();
    expect(branchesOf(tab).selected).toBe(other.name);
    await userEvent.click(screen.getByRole('button', { name: 'Volver al detalle' }));
    await waitFor(() => expect(screen.queryByRole('region', { name: 'Detalle de la rama' })).toBeNull());
    expect(screen.getByRole('region', { name: 'Diferencias del archivo' })).toBeInTheDocument();
  });

  it('Ctrl+B contrae el riel a iconos y lo vuelve a expandir', async () => {
    await setup();
    expect(runShortcut('rail.toggle')).toBe(true);
    await waitFor(() => expect(screen.getByRole('button', { name: 'Expandir el riel de ramas' })).toBeInTheDocument());
    expect(screen.queryByRole('navigation', { name: 'Árbol de ramas' })).toBeNull();
    expect(runShortcut('rail.toggle')).toBe(true);
    await waitFor(() => expect(screen.getByRole('navigation', { name: 'Árbol de ramas' })).toBeInTheDocument());
  });

  it('abre remotos y etiquetas desde el riel', async () => {
    await setup();
    await userEvent.click(screen.getByRole('button', { name: 'Remotos y etiquetas' }));
    expect(await screen.findByRole('dialog', { name: 'Remotos y etiquetas' })).toBeInTheDocument();
    expect(await screen.findByRole('region', { name: 'Remotos del repositorio' })).toBeInTheDocument();
  });
});
