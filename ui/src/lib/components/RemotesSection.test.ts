import { render, screen, waitFor, within } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { branchesOf } from '$lib/stores/branches.svelte';
import { loadedTab, resetGlobals } from '../../test/helpers';
import BranchDetail from './BranchDetail.svelte';
import RemotesSection from './RemotesSection.svelte';
import Toasts from './Toasts.svelte';

beforeEach(resetGlobals);
afterEach(resetGlobals);

async function setup(opts = {}, tabId = 'tab-ariadna') {
  const ctx = await loadedTab(tabId, opts);
  render(RemotesSection, { props: { tab: ctx.tab } });
  render(Toasts);
  return ctx;
}

describe('Remotos', () => {
  it('lista el remoto con proveedor, host y URL', async () => {
    await setup();
    const region = await screen.findByRole('region', { name: 'Remotos del repositorio' });
    const origin = await within(region).findByText('origin');
    const card = origin.closest('li')!;
    expect(within(card).getByText(/GitHub · github\.com · SSH/)).toBeInTheDocument();
    expect(within(card).getByText('git@github.com:javie/ariadna.git')).toBeInTheDocument();
    expect(card.querySelector('[data-provider="github"]')).not.toBeNull();
  });

  it('«Abrir el repositorio en el navegador» pasa por Rust con remoto y tipo', async () => {
    const { api } = await setup();
    await userEvent.click(await screen.findByRole('button', { name: 'Abrir el repositorio en el navegador' }));
    await waitFor(() => expect(api.__openedUrls()).toEqual(['https://github.com/javie/ariadna']));
  });

  it('un remoto sin página web conocida deja el botón de abrir deshabilitado', async () => {
    await setup({}, 'tab-legacy');
    const open = await screen.findByRole('button', { name: 'Abrir el repositorio en el navegador' });
    expect(open).toBeDisabled();
  });

  it('añadir: valida el nombre, pide la URL y publica el remoto tras la confirmación de Rust', async () => {
    const { tab } = await setup();
    await userEvent.click(await screen.findByRole('button', { name: 'Añadir remoto' }));
    const name = screen.getByLabelText('Nombre');
    const url = screen.getByLabelText('Dirección (URL)');
    const submit = screen.getByRole('button', { name: 'Añadir' });
    await userEvent.clear(name);
    await userEvent.type(name, 'con espacio');
    expect(screen.getByText('El nombre no puede tener espacios.')).toBeInTheDocument();
    expect(submit).toBeDisabled();
    await userEvent.clear(name);
    await userEvent.type(name, 'azure');
    await userEvent.type(url, 'https://dev.azure.com/org/proj/_git/app');
    expect(submit).toBeEnabled();
    await userEvent.click(submit);
    expect(await screen.findByText('Remoto «azure» añadido')).toBeInTheDocument();
    await waitFor(() => expect(tab.snapshot!.remotes.map((r) => r.name)).toContain('azure'));
    const card = (await screen.findByText('azure', { selector: '.rname' })).closest('li')!;
    expect(within(card).getByText(/Azure DevOps · dev\.azure\.com/)).toBeInTheDocument();
  });

  it('añadir con contraseña en la URL se rechaza con un aviso y no cambia nada', async () => {
    const { tab } = await setup();
    await userEvent.click(await screen.findByRole('button', { name: 'Añadir remoto' }));
    await userEvent.clear(screen.getByLabelText('Nombre'));
    await userEvent.type(screen.getByLabelText('Nombre'), 'malo');
    await userEvent.type(screen.getByLabelText('Dirección (URL)'), 'https://u:pw@github.com/a/b.git');
    await userEvent.click(screen.getByRole('button', { name: 'Añadir' }));
    expect(await screen.findByText('No se pudo añadir el remoto')).toBeInTheDocument();
    expect(tab.snapshot!.remotes.map((r) => r.name)).toEqual(['origin']);
  });

  it('cancelar la confirmación nativa deja el remoto como estaba', async () => {
    const { tab } = await setup({ confirm: () => false });
    await userEvent.click(await screen.findByRole('button', { name: 'Quitar remoto' }));
    await new Promise((r) => setTimeout(r, 20));
    expect(tab.snapshot!.remotes.map((r) => r.name)).toEqual(['origin']);
    expect(screen.queryByText('Remoto «origin» quitado')).not.toBeInTheDocument();
  });

  it('renombrar y quitar', async () => {
    const { tab } = await setup();
    await userEvent.click(await screen.findByRole('button', { name: 'Renombrar remoto' }));
    const input = screen.getByLabelText('Nombre nuevo');
    await userEvent.clear(input);
    await userEvent.type(input, 'principal');
    await userEvent.click(screen.getByRole('button', { name: 'Renombrar' }));
    expect(await screen.findByText('Remoto «origin» renombrado a «principal»')).toBeInTheDocument();
    await waitFor(() => expect(tab.snapshot!.remotes[0]!.name).toBe('principal'));
    await userEvent.click(await screen.findByRole('button', { name: 'Quitar remoto' }));
    expect(await screen.findByText('Remoto «principal» quitado')).toBeInTheDocument();
    expect(await screen.findByText(/no tiene remotos/)).toBeInTheDocument();
  });

  it('traer, bajar y subir la rama actual', async () => {
    await setup();
    await userEvent.click(await screen.findByRole('button', { name: 'Traer (fetch)' }));
    expect(await screen.findByText('Remoto «origin» actualizado')).toBeInTheDocument();
    await userEvent.click(screen.getByRole('button', { name: 'Bajar rama actual' }));
    expect(await screen.findByText('Ya estabas al día con «origin»')).toBeInTheDocument();
    await userEvent.click(screen.getByRole('button', { name: 'Subir rama actual' }));
    expect(await screen.findByText(/subida a «origin»/)).toBeInTheDocument();
  });
});

describe('Ramas: abrir en el navegador y crear pull request', () => {
  async function withDetail(opts = {}, tabId = 'tab-ariadna') {
    const ctx = await loadedTab(tabId, opts);
    render(BranchDetail, { props: { tab: ctx.tab } });
    render(Toasts);
    const detail = await screen.findByRole('region', { name: 'Detalle de la rama' });
    return { ...ctx, detail };
  }

  it('la rama actual abre su página y la de nueva pull request en GitHub', async () => {
    const { api, tab, detail } = await withDetail();
    const head = tab.snapshot!.branches.find((b) => b.isHead)!.name;
    const open = await within(detail).findByRole('button', { name: 'Abrir rama en el navegador' });
    await waitFor(() => expect(open).toBeEnabled());
    await userEvent.click(open);
    await userEvent.click(within(detail).getByRole('button', { name: 'Crear pull request' }));
    await waitFor(() => expect(api.__openedUrls()).toHaveLength(2));
    expect(api.__openedUrls()[0]).toBe(`https://github.com/javie/ariadna/tree/${head}`);
    expect(api.__openedUrls()[1]).toBe(`https://github.com/javie/ariadna/pull/new/${head}`);
    expect(within(detail).getByText(/Enlaces vía «origin» \(GitHub\)/)).toBeInTheDocument();
  });

  it('una rama remota usa su propio remoto y el nombre sin prefijo', async () => {
    const { api, tab, detail } = await withDetail();
    const remote = tab.snapshot!.remoteBranches.find((b) => b.name === 'origin/main')!;
    branchesOf(tab).select(remote.name);
    const pr = await within(detail).findByRole('button', { name: 'Crear pull request' });
    await waitFor(() => expect(pr).toBeEnabled());
    await userEvent.click(pr);
    await waitFor(() => expect(api.__openedUrls().length).toBeGreaterThan(0));
    expect(api.__openedUrls()).toEqual(['https://github.com/javie/ariadna/pull/new/main']);
  });

  it('con un remoto sin página web conocida los dos botones quedan deshabilitados y se explica', async () => {
    const { detail } = await withDetail({}, 'tab-legacy');
    await waitFor(() => expect(within(detail).getByText(/Sin remoto con página web/)).toBeInTheDocument());
    expect(within(detail).getByRole('button', { name: 'Crear pull request' })).toBeDisabled();
    expect(within(detail).getByRole('button', { name: 'Abrir rama en el navegador' })).toBeDisabled();
  });
});
