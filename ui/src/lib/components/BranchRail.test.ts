import { render, screen, waitFor, within } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { branchesOf } from '$lib/stores/branches.svelte';
import { runShortcut } from '$lib/shortcuts/registry';
import { loadedTab, resetGlobals } from '../../test/helpers';
import BranchRail from './BranchRail.svelte';
import Toasts from './Toasts.svelte';

beforeEach(resetGlobals);
afterEach(() => {
  resetGlobals();
  vi.restoreAllMocks();
});

type RailProps = { collapsed?: boolean; oninspect?: (name: string) => void; onrepo?: () => void; ons?: () => void };

async function setup(opts = {}, props: RailProps = {}) {
  const ctx = await loadedTab('tab-ariadna', opts);
  const ontoggle = vi.fn();
  const oninspect = vi.fn();
  const onrepo = vi.fn();
  render(BranchRail, {
    props: { tab: ctx.tab, collapsed: props.collapsed ?? false, ontoggle, oninspect: props.oninspect ?? oninspect, onrepo: props.onrepo ?? onrepo }
  });
  render(Toasts);
  return { ...ctx, ontoggle, oninspect, onrepo };
}

const branchNames = (tab: Awaited<ReturnType<typeof setup>>['tab']) => (tab.snapshot?.branches ?? []).map((b) => b.name);

/** Fila de rama por nombre exacto (`main` y `origin/main` conviven). */
function row(name: string): HTMLElement {
  const el = [...document.querySelectorAll<HTMLElement>('[data-brail-name]')].find((b) => b.dataset.brailName === name);
  if (!el) throw new Error(`no hay fila para ${name}`);
  return el;
}

const current = (tab: Awaited<ReturnType<typeof setup>>['tab']) => tab.snapshot!.branches.find((b) => b.isHead)!;

describe('Riel de ramas: lista y jerarquía', () => {
  it('agrupa locales y remotas, destaca la actual y muestra el inspector', async () => {
    const { tab } = await setup();
    const nav = await screen.findByRole('navigation', { name: 'Árbol de ramas' });
    expect(within(nav).getByRole('button', { name: /Locales/ })).toBeInTheDocument();
    expect(within(nav).getByRole('button', { name: /Remotas/ })).toBeInTheDocument();
    const cur = current(tab);
    expect(within(nav).getByText('(rama actual)', { exact: false })).toBeInTheDocument();
    expect(row(cur.name)).toHaveAttribute('aria-current', 'true');
    // Sin selección explícita no hay inspector: la tarjeta superior ya resume la rama actual.
    expect(screen.queryByRole('group', { name: 'Rama seleccionada' })).toBeNull();

    await userEvent.click(row(cur.name));
    const inspector = screen.getByRole('group', { name: 'Rama seleccionada' });
    expect(within(inspector).getByText(cur.name)).toBeInTheDocument();
    expect(within(inspector).getByRole('button', { name: 'Cambiar a esta rama' })).toBeDisabled();
    expect(within(inspector).getByRole('button', { name: 'Borrar' })).toBeDisabled();
    await userEvent.click(within(inspector).getByRole('button', { name: 'Quitar la selección' }));
    expect(screen.queryByRole('group', { name: 'Rama seleccionada' })).toBeNull();
  });

  it('filtra al instante y explica cuando no hay coincidencias', async () => {
    await setup();
    const filter = screen.getByLabelText('Buscar rama');
    await userEvent.type(filter, 'feature');
    expect(row('feature/command-palette')).toBeInTheDocument();
    expect(document.querySelector('[data-brail-name="main"]')).toBeNull();
    await userEvent.clear(filter);
    await userEvent.type(filter, 'zzz');
    expect((await screen.findAllByText('Ninguna rama coincide con «zzz».')).length).toBeGreaterThan(0);
  });

  it('Intro en el filtro cambia a la primera rama que coincide (con aviso si hay cambios)', async () => {
    const { tab } = await setup();
    const filter = screen.getByLabelText('Buscar rama');
    await userEvent.type(filter, 'feature{Enter}');
    expect(await screen.findByText('Tienes cambios sin confirmar')).toBeInTheDocument();
    await userEvent.click(screen.getByRole('button', { name: 'Cambiar de todos modos' }));
    await waitFor(() => expect(tab.snapshot!.branches.find((b) => b.isHead)?.name).toBe('feature/command-palette'));
  });

  it('pliega los grupos por prefijo', async () => {
    await setup();
    await screen.findByRole('navigation', { name: 'Árbol de ramas' });
    const group = [...document.querySelectorAll<HTMLElement>('button.subhead')].find((b) => b.textContent?.trim().startsWith('feature/'))!;
    expect(group).toHaveAttribute('aria-expanded', 'true');
    expect(row('feature/command-palette')).toBeInTheDocument();
    await userEvent.click(group);
    expect(group).toHaveAttribute('aria-expanded', 'false');
    expect(document.querySelector('[data-brail-name="feature/command-palette"]')).toBeNull();
  });

  it('«Detalle» pide abrir la rama seleccionada en el panel derecho', async () => {
    const { oninspect } = await setup();
    const other = row('main');
    await userEvent.click(other);
    await userEvent.click(screen.getByRole('button', { name: /^Detalle de / }));
    expect(oninspect).toHaveBeenCalledWith('main');
  });
});

describe('Riel de ramas: cambiar, crear, renombrar, borrar y fusionar', () => {
  it('cambiar de rama con cambios avisa antes y «Cambiar de todos modos» hace el checkout', async () => {
    const { tab, api } = await setup();
    expect(tab.info.changeCount).toBeGreaterThan(0);
    const spy = vi.spyOn(api, 'checkout');
    const other = tab.snapshot!.branches.find((b) => !b.isHead)!;
    await userEvent.click(row(other.name));
    await userEvent.click(screen.getByRole('button', { name: 'Cambiar a esta rama' }));
    expect(await screen.findByText('Tienes cambios sin confirmar')).toBeInTheDocument();
    expect(spy).not.toHaveBeenCalledWith(tab.id, other.name);
    await userEvent.click(screen.getByRole('button', { name: 'Cambiar de todos modos' }));
    await waitFor(() => expect(spy).toHaveBeenCalledWith(tab.id, other.name));
    expect(await screen.findByText(`Ahora estás en «${other.name}»`)).toBeInTheDocument();
  });

  it('crea una rama con validación inmediata del nombre', async () => {
    const { tab } = await setup();
    await userEvent.click(screen.getByRole('button', { name: 'Nueva rama' }));
    const name = screen.getByLabelText('Nombre');
    const create = screen.getByRole('button', { name: 'Crear rama' });
    await userEvent.type(name, 'con espacio');
    expect(screen.getByText('El nombre no puede tener espacios.')).toBeInTheDocument();
    expect(create).toBeDisabled();
    await userEvent.clear(name);
    await userEvent.type(name, 'feature/nueva');
    expect(create).toBeEnabled();
    await userEvent.click(create);
    await waitFor(() => expect(branchNames(tab)).toContain('feature/nueva'));
    expect(await screen.findByText('Rama «feature/nueva» creada')).toBeInTheDocument();
    expect(screen.getByRole('group', { name: 'Rama seleccionada' })).toBeInTheDocument();
  });

  it('renombra la rama seleccionada con validación', async () => {
    const { tab } = await setup();
    const b = branchesOf(tab);
    await userEvent.click(row(b.current!.name));
    await userEvent.click(await screen.findByRole('button', { name: 'Renombrar' }));
    const input = screen.getByLabelText('Nuevo nombre');
    expect(input).toHaveValue(b.current!.name);
    await userEvent.clear(input);
    await userEvent.type(input, 'nombre..malo');
    expect(screen.getByText(/caracteres no permitidos/)).toBeInTheDocument();
    await userEvent.clear(input);
    await userEvent.type(input, 'renombrada');
    await userEvent.click(screen.getByRole('button', { name: 'Renombrar' }));
    await waitFor(() => expect(branchNames(tab)).toContain('renombrada'));
  });

  it('borrar pasa por la confirmación nativa', async () => {
    let answer = false;
    const { tab, api } = await setup({ confirm: () => answer });
    await api.createBranch(tab.id, 'temporal', null);
    await waitFor(() => expect(branchNames(tab)).toContain('temporal'));
    await userEvent.click(row('temporal'));
    await userEvent.click(screen.getByRole('button', { name: 'Borrar' }));
    await waitFor(() => expect(branchesOf(tab).busy).toBeNull());
    expect(branchNames(tab)).toContain('temporal');
    answer = true;
    await userEvent.click(screen.getByRole('button', { name: 'Borrar' }));
    await waitFor(() => expect(branchNames(tab)).not.toContain('temporal'));
    expect(await screen.findByText('Rama «temporal» borrada')).toBeInTheDocument();
  });

  it('fusionar pide confirmación; con conflictos deja el merge en curso y abre Conflictos', async () => {
    const { tab, api } = await setup();
    await api.createBranch(tab.id, 'limpia', null);
    await api.createBranch(tab.id, 'choca-con-main', null);
    await waitFor(() => expect(branchNames(tab)).toContain('choca-con-main'));

    await userEvent.click(row('limpia'));
    await userEvent.click(screen.getByRole('button', { name: /^Fusionar en / }));
    expect(screen.getByText(/la fusión queda en curso/)).toBeInTheDocument();
    await userEvent.click(screen.getByRole('button', { name: 'Fusionar' }));
    expect(await screen.findByText(/«limpia» (fusionada|ya estaba fusionada)/)).toBeInTheDocument();

    await userEvent.click(row('choca-con-main'));
    await userEvent.click(screen.getByRole('button', { name: /^Fusionar en / }));
    await userEvent.click(screen.getByRole('button', { name: 'Fusionar' }));
    expect(await screen.findByText('Fusión con conflictos: resuélvelos en el panel Conflictos')).toBeInTheDocument();
    await waitFor(() => expect(tab.snapshot!.integration).toBe('merging'));
    expect(tab.panel).toBe('conflicts');
  });

  it('una rama remota ofrece crear la local y no se puede renombrar ni borrar', async () => {
    const { tab } = await setup();
    const remote = tab.snapshot!.remoteBranches[0]!;
    await userEvent.click(row(remote.name));
    expect(screen.getByRole('button', { name: 'Crear rama local y cambiar' })).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Borrar' })).not.toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Renombrar' })).not.toBeInTheDocument();
  });
});

describe('Riel de ramas: teclado y contracción', () => {
  it('Ctrl+Shift+B lleva el foco al buscador de ramas', async () => {
    await setup();
    await screen.findByRole('navigation', { name: 'Árbol de ramas' });
    expect(runShortcut('rail.focus')).toBe(true);
    await waitFor(() => expect(document.activeElement).toBe(screen.getByLabelText('Buscar rama')));
  });

  it('las flechas mueven el foco por el árbol y Esc limpia el filtro', async () => {
    await setup();
    const filter = screen.getByLabelText('Buscar rama');
    await userEvent.type(filter, 'main');
    await userEvent.keyboard('{ArrowDown}');
    const first = document.activeElement as HTMLElement;
    expect(first.dataset.brailName).toBeTruthy();
    const before = first.dataset.brailName;
    await userEvent.keyboard('{ArrowDown}');
    expect((document.activeElement as HTMLElement).dataset.brailName).not.toBe(before);
    await userEvent.click(filter);
    await userEvent.keyboard('{Escape}');
    expect(filter).toHaveValue('');
  });

  it('contraído muestra solo iconos con nombres accesibles', async () => {
    const { ontoggle } = await setup({}, { collapsed: true });
    expect(await screen.findByRole('button', { name: 'Expandir el riel de ramas' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Nueva rama' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /Rama actual: / })).toBeInTheDocument();
    expect(screen.queryByRole('navigation', { name: 'Árbol de ramas' })).toBeNull();
    await userEvent.click(screen.getByRole('button', { name: 'Expandir el riel de ramas' }));
    expect(ontoggle).toHaveBeenCalled();
  });

  it('el botón de contraer anuncia el estado y llama a ontoggle', async () => {
    const { ontoggle } = await setup();
    await userEvent.click(await screen.findByRole('button', { name: 'Contraer el riel de ramas' }));
    expect(ontoggle).toHaveBeenCalled();
  });
});
