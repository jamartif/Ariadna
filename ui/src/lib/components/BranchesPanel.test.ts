import { render, screen, waitFor, within } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { branchesOf } from '$lib/stores/branches.svelte';
import { loadedTab, resetGlobals } from '../../test/helpers';
import BranchDetail from './BranchDetail.svelte';
import BranchesPanel from './BranchesPanel.svelte';
import Toasts from './Toasts.svelte';

beforeEach(resetGlobals);
afterEach(resetGlobals);

async function setup(opts = {}) {
  const ctx = await loadedTab('tab-ariadna', opts);
  render(BranchesPanel, { props: { tab: ctx.tab } });
  render(BranchDetail, { props: { tab: ctx.tab } });
  render(Toasts);
  return ctx;
}

/** Fila de rama por nombre exacto (hay `main` y `origin/main`). */
function row(name: string): HTMLElement {
  const el = [...document.querySelectorAll<HTMLElement>('button.item')].find((b) => b.querySelector('.name')?.textContent === name);
  if (!el) throw new Error(`no hay fila para ${name}`);
  return el;
}

const branchNames = (tab: Awaited<ReturnType<typeof setup>>['tab']) => (tab.snapshot?.branches ?? []).map((b) => b.name);

describe('Ramas', () => {
  it('lista locales y remotas, marca la actual y muestra su detalle', async () => {
    const { tab } = await setup();
    expect(await screen.findByRole('heading', { name: /Locales/ })).toBeInTheDocument();
    expect(screen.getByRole('heading', { name: /Remotas/ })).toBeInTheDocument();
    const cur = tab.snapshot!.branches.find((b) => b.isHead)!;
    const detail = screen.getByRole('region', { name: 'Detalle de la rama' });
    expect(within(detail).getByRole('heading', { level: 2, name: cur.name })).toBeInTheDocument();
    expect(within(detail).getByRole('button', { name: 'Cambiar a esta rama' })).toBeDisabled();
    expect(within(detail).getByRole('button', { name: 'Borrar' })).toBeDisabled();
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
  });

  it('cambiar de rama con cambios avisa antes y respeta «cancelar»', async () => {
    const { tab, api } = await setup();
    expect(tab.info.changeCount).toBeGreaterThan(0);
    const spy = vi.spyOn(api, 'checkout');
    const other = tab.snapshot!.branches.find((b) => !b.isHead)!;
    await userEvent.click(row(other.name));
    await userEvent.click(screen.getByRole('button', { name: 'Cambiar a esta rama' }));
    expect(await screen.findByText('Tienes cambios sin confirmar')).toBeInTheDocument();
    expect(spy).not.toHaveBeenCalled();
    await userEvent.click(screen.getAllByRole('button', { name: 'Cancelar' }).at(-1)!);
    expect(screen.queryByText('Tienes cambios sin confirmar')).not.toBeInTheDocument();
    expect(spy).not.toHaveBeenCalled();
  });

  it('«Cambiar de todos modos» hace el checkout y avisa', async () => {
    const { tab, api } = await setup();
    const spy = vi.spyOn(api, 'checkout');
    const other = tab.snapshot!.branches.find((b) => !b.isHead)!;
    await userEvent.click(row(other.name));
    await userEvent.click(screen.getByRole('button', { name: 'Cambiar a esta rama' }));
    await userEvent.click(await screen.findByRole('button', { name: 'Cambiar de todos modos' }));
    await waitFor(() => expect(spy).toHaveBeenCalledWith(tab.id, other.name));
    await waitFor(() => expect(tab.snapshot!.branches.find((b) => b.isHead)?.name).toBe(other.name));
    expect(await screen.findByText(`Ahora estás en «${other.name}»`)).toBeInTheDocument();
  });

  it('renombrar valida y aplica', async () => {
    const { tab } = await setup();
    const b = branchesOf(tab);
    await userEvent.click(await screen.findByRole('button', { name: 'Renombrar' }));
    const input = screen.getByLabelText('Nuevo nombre');
    expect(input).toHaveValue(b.current!.name);
    await userEvent.clear(input);
    await userEvent.type(input, 'nombre..malo');
    expect(screen.getByText(/caracteres no permitidos/)).toBeInTheDocument();
    await userEvent.clear(input);
    await userEvent.type(input, 'renombrada');
    await userEvent.click(document.querySelector<HTMLButtonElement>('form.rename button[type="submit"]')!);
    await waitFor(() => expect(branchNames(tab)).toContain('renombrada'));
  });

  it('borrar pasa por la confirmación nativa: si se rechaza no se borra, si se acepta sí', async () => {
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

  it('fusionar sin conflictos funciona; con conflictos se explica y no se cambia nada', async () => {
    const { tab, api } = await setup();
    await api.createBranch(tab.id, 'limpia', null);
    await api.createBranch(tab.id, 'choca-con-main', null);
    await waitFor(() => expect(branchNames(tab)).toContain('choca-con-main'));

    await userEvent.click(row('limpia'));
    await userEvent.click(screen.getByRole('button', { name: /^Fusionar en/ }));
    expect(screen.getByText(/Solo se fusiona si no hay conflictos/)).toBeInTheDocument();
    await userEvent.click(screen.getByRole('button', { name: 'Fusionar' }));
    expect(await screen.findByText(/«limpia» (fusionada|ya estaba fusionada)/)).toBeInTheDocument();

    await userEvent.click(row('choca-con-main'));
    await userEvent.click(screen.getByRole('button', { name: /^Fusionar en/ }));
    await userEvent.click(screen.getByRole('button', { name: 'Fusionar' }));
    expect(await screen.findByText('Hay conflictos: no se ha fusionado')).toBeInTheDocument();
    expect(screen.getByText('docs/ipc.md')).toBeInTheDocument();
    expect(tab.snapshot!.integration).toBe('none');
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
