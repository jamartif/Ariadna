import { render, screen, waitFor } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { ApiError } from '$lib/api/types';
import { runShortcut } from '$lib/shortcuts/registry';
import { toasts } from '$lib/stores/toasts.svelte';
import type { StatusEntry } from '$lib/api/types';
import type { SectionId } from '$lib/util/changes';
import type { TabState } from '$lib/stores/tab.svelte';
import { loadedTab, resetGlobals } from '../../test/helpers';
import DiffView from './DiffView.svelte';

beforeEach(resetGlobals);
afterEach(resetGlobals);

const TYPES = 'src/lib/api/types.ts';

function pick(tab: TabState, section: SectionId, path: string): StatusEntry {
  const e = tab.sections[section].find((x) => x.path === path);
  if (!e) throw new Error(`${path} no está en ${section}`);
  tab.select(section, e);
  return e;
}

const boxes = () => screen.findAllByRole('checkbox', { name: /^Seleccionar la línea/ });

describe('DiffView: líneas sueltas', () => {
  it('cada línea que cambia tiene su casilla; el contexto no', async () => {
    const { tab } = await loadedTab();
    pick(tab, 'unstaged', TYPES);
    render(DiffView, { props: { tab } });
    const cbs = await boxes();
    // hunk 1: 1 borrada + 1 añadida; hunk 2: 2 añadidas
    expect(cbs).toHaveLength(4);
    expect(cbs[0]).toHaveAccessibleName(/eliminada 17: export type ChangeStatus/);
    expect(cbs[1]).toHaveAccessibleName(/añadida 17: export type ChangeStatus/);
    // Un solo tabstop (roving): solo la primera casilla es tabulable.
    expect(cbs.filter((c) => c.tabIndex === 0)).toHaveLength(1);
  });

  it('elegir una línea muestra «Preparar líneas seleccionadas (1)» y solo prepara esa', async () => {
    const { tab, api } = await loadedTab();
    const entry = pick(tab, 'unstaged', TYPES);
    render(DiffView, { props: { tab } });
    const cbs = await boxes();
    await userEvent.click(cbs[1] as HTMLElement); // la añadida del hunk 1
    expect(cbs[1]).toBeChecked();
    expect(screen.getByRole('status')).toHaveTextContent('1 línea seleccionada');
    const apply = await screen.findAllByRole('button', { name: 'Preparar líneas seleccionadas (1)' });
    await userEvent.click(apply[0] as HTMLElement);

    await waitFor(async () => {
      const staged = await api.diff(tab.id, 'staged', [entry.path]);
      expect(staged[0]!.hunks.some((h) => h.lines.some((l) => l.kind === 'addition' && l.text.includes("'R' | 'C'")))).toBe(true);
    });
    const unstaged = await api.diff(tab.id, 'unstaged', [entry.path]);
    const text = unstaged[0]!.hunks.flatMap((h) => h.lines.filter((l) => l.kind !== 'context').map((l) => `${l.kind}:${l.text}`));
    // La borrada sigue sin preparar; la añadida ya no está entre las pendientes.
    expect(text.some((x) => x.startsWith('deletion:export type ChangeStatus'))).toBe(true);
    expect(text.some((x) => x.startsWith('addition:export type ChangeStatus'))).toBe(false);
    // La selección se limpia tras aplicar.
    await waitFor(() => expect(screen.queryByRole('button', { name: /^Preparar líneas seleccionadas/ })).not.toBeInTheDocument());
  });

  it('Mayús+clic selecciona un rango dentro del hunk', async () => {
    const { tab } = await loadedTab();
    pick(tab, 'unstaged', TYPES);
    render(DiffView, { props: { tab } });
    const cbs = await boxes();
    await userEvent.click(cbs[0] as HTMLElement);
    await userEvent.keyboard('{Shift>}');
    await userEvent.click(cbs[1] as HTMLElement);
    await userEvent.keyboard('{/Shift}');
    expect(cbs[0]).toBeChecked();
    expect(cbs[1]).toBeChecked();
    expect(screen.getByRole('status')).toHaveTextContent('2 líneas seleccionadas');
    expect((await screen.findAllByRole('button', { name: 'Preparar líneas seleccionadas (2)' })).length).toBeGreaterThan(0);
  });

  it('elegir en otro hunk sustituye la selección (nunca se mezclan hunks)', async () => {
    const { tab } = await loadedTab();
    pick(tab, 'unstaged', TYPES);
    render(DiffView, { props: { tab } });
    const cbs = await boxes();
    await userEvent.click(cbs[0] as HTMLElement);
    await userEvent.click(cbs[2] as HTMLElement); // hunk 2
    expect(cbs[0]).not.toBeChecked();
    expect(cbs[2]).toBeChecked();
    expect(screen.getByRole('status')).toHaveTextContent('1 línea seleccionada');
  });

  it('el teclado elige y mueve: Espacio, ↓, Mayús+↓ y Esc', async () => {
    const { tab } = await loadedTab();
    pick(tab, 'unstaged', TYPES);
    render(DiffView, { props: { tab } });
    const cbs = await boxes();
    cbs[0]!.focus();
    await userEvent.keyboard(' ');
    expect(cbs[0]).toBeChecked();
    await userEvent.keyboard('{ArrowDown}');
    await waitFor(() => expect(cbs[1]).toHaveFocus());
    expect(cbs[1]).not.toBeChecked();
    await userEvent.keyboard('{Shift>}{ArrowDown}{/Shift}');
    await waitFor(() => expect(screen.getByRole('status')).toHaveTextContent(/líneas? seleccionadas?/));
    await userEvent.keyboard('{Escape}');
    await waitFor(() => expect(cbs[0]).not.toBeChecked());
    expect(screen.getByRole('status')).toHaveTextContent('');
  });

  it('el atajo «lines.apply» (Mod+Mayús+Intro) aplica la selección', async () => {
    const { tab, api } = await loadedTab();
    const entry = pick(tab, 'unstaged', TYPES);
    render(DiffView, { props: { tab } });
    const cbs = await boxes();
    expect(runShortcut('lines.apply')).toBe(false); // sin selección declina
    await userEvent.click(cbs[3] as HTMLElement);
    expect(runShortcut('lines.apply')).toBe(true);
    await waitFor(async () => {
      const staged = await api.diff(tab.id, 'staged', [entry.path]);
      expect(staged[0]!.hunks.some((h) => h.lines.some((l) => l.text.includes('conflicts: ConflictFile[]')))).toBe(true);
    });
  });

  it('en Preparados el botón es «Quitar de preparados (n)» y solo quita esa línea', async () => {
    const { tab, api } = await loadedTab();
    const entry = pick(tab, 'staged', TYPES);
    render(DiffView, { props: { tab } });
    const cbs = await boxes();
    expect(cbs.length).toBeGreaterThan(2);
    await userEvent.click(cbs[cbs.length - 1] as HTMLElement);
    const btn = (await screen.findAllByRole('button', { name: 'Quitar de preparados (1)' }))[0] as HTMLElement;
    await userEvent.click(btn);
    await waitFor(async () => {
      const unstaged = await api.diff(tab.id, 'unstaged', [entry.path]);
      expect(unstaged[0]!.hunks.length).toBeGreaterThan(2);
    });
    const staged = await api.diff(tab.id, 'staged', [entry.path]);
    expect(staged[0]!.hunks.length).toBeGreaterThan(0);
  });

  it('un rechazo claro (PATCH_REJECTED) se avisa, no cambia nada y fuerza un refresco', async () => {
    const { tab, api } = await loadedTab();
    pick(tab, 'unstaged', TYPES);
    render(DiffView, { props: { tab } });
    const cbs = await boxes();
    await userEvent.click(cbs[1] as HTMLElement);
    const refresh = vi.spyOn(api, 'refresh');
    api.stageLines = async () => {
      throw new ApiError({ code: 'PATCH_REJECTED', message: 'No se pueden preparar esas líneas: el parche no aplica.' });
    };
    await userEvent.click((await screen.findAllByRole('button', { name: 'Preparar líneas seleccionadas (1)' }))[0] as HTMLElement);
    await waitFor(() => expect(toasts.items.some((x) => x.title === 'No se pudieron preparar las líneas')).toBe(true));
    expect(toasts.items.find((x) => x.title === 'No se pudieron preparar las líneas')!.message).toContain('el parche no aplica');
    await waitFor(() => expect(refresh).toHaveBeenCalled());
  });

  it('limpiar la selección con el botón de la barra del hunk', async () => {
    const { tab } = await loadedTab();
    pick(tab, 'unstaged', TYPES);
    render(DiffView, { props: { tab } });
    const cbs = await boxes();
    await userEvent.click(cbs[2] as HTMLElement);
    await userEvent.click((await screen.findAllByRole('button', { name: 'Limpiar la selección de líneas' }))[0] as HTMLElement);
    expect(cbs[2]).not.toBeChecked();
    expect(screen.queryByRole('button', { name: /^Preparar líneas seleccionadas/ })).not.toBeInTheDocument();
  });

  it('un archivo nuevo sin seguimiento también permite elegir líneas', async () => {
    const { tab, api } = await loadedTab();
    const entry = pick(tab, 'untracked', 'notas.txt');
    render(DiffView, { props: { tab } });
    const cbs = await boxes();
    expect(cbs.length).toBeGreaterThan(2);
    await userEvent.click(cbs[1] as HTMLElement);
    await userEvent.click((await screen.findAllByRole('button', { name: 'Preparar líneas seleccionadas (1)' }))[0] as HTMLElement);
    await waitFor(async () => {
      const staged = await api.diff(tab.id, 'staged', [entry.path]);
      expect(staged[0]!.hunks.flatMap((h) => h.lines).filter((l) => l.kind === 'addition')).toHaveLength(1);
    });
  });

  it('los binarios no ofrecen casillas', async () => {
    const { tab } = await loadedTab();
    pick(tab, 'unstaged', 'docs/screenshots/staging.png');
    render(DiffView, { props: { tab } });
    await screen.findByText('Archivo binario');
    expect(screen.queryAllByRole('checkbox')).toHaveLength(0);
  });
});
