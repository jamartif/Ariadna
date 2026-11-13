import { render, screen, waitFor } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { CloneFlow } from '$lib/stores/clone.svelte';
import { mockApi, readyApp, resetGlobals } from '../../test/helpers';
import CloneDialog from './CloneDialog.svelte';

beforeEach(resetGlobals);
afterEach(resetGlobals);

const PAT = 'ghp_0123456789abcdef';

async function setup() {
  const ctx = await readyApp();
  ctx.app.newTab();
  const onclose = vi.fn();
  const cloneSpy = vi.spyOn(ctx.api, 'cloneRepository');
  const storeSpy = vi.spyOn(ctx.api, 'storeCredential');
  render(CloneDialog, { props: { app: ctx.app, onclose } });
  return { ...ctx, onclose, cloneSpy, storeSpy };
}

async function fill(url: string, dest = '/tmp/destino/repo') {
  await userEvent.type(screen.getByLabelText('URL del repositorio'), url);
  await userEvent.type(screen.getByLabelText('Carpeta de destino'), dest);
}

describe('Clonar repositorio', () => {
  it('reconoce el proveedor y sugiere el nombre de la carpeta', async () => {
    await setup();
    await userEvent.type(screen.getByLabelText('URL del repositorio'), 'https://dev.azure.com/org/proyecto/_git/mi-repo');
    expect(screen.getByText('Azure DevOps')).toBeInTheDocument();
    expect(screen.getByText('org / proyecto / mi-repo')).toBeInTheDocument();
  });

  it('no deja clonar con una URL vacía, inválida o con contraseña incrustada', async () => {
    await setup();
    const url = screen.getByLabelText('URL del repositorio');
    const submit = screen.getByRole('button', { name: 'Clonar' });
    expect(submit).toBeDisabled();
    await userEvent.type(url, 'esto no es una url');
    expect(screen.getByText(/No parece una URL de Git válida/)).toBeInTheDocument();
    await userEvent.clear(url);
    await userEvent.type(url, 'https://usuario:clave@github.com/o/r.git');
    expect(screen.getByText(/contraseña incrustada/)).toBeInTheDocument();
    expect(submit).toBeDisabled();
  });

  it('un repo privado sin token da AUTH y abre el formulario del PAT', async () => {
    const { cloneSpy, onclose } = await setup();
    await fill('https://github.com/org/private-repo.git');
    await userEvent.click(screen.getByRole('button', { name: 'Clonar' }));
    expect(await screen.findByText('No se pudo autenticar')).toBeInTheDocument();
    expect(cloneSpy).toHaveBeenCalledWith('https://github.com/org/private-repo.git', '/tmp/destino/repo', null, null);
    expect(screen.getByLabelText('Token')).toBeInTheDocument();
    expect(onclose).not.toHaveBeenCalled();
  });

  it('con PAT: se envía una sola vez a clone_repository, se borra de la interfaz y no se guarda', async () => {
    const { cloneSpy, storeSpy, onclose, app } = await setup();
    await fill('https://github.com/org/private-repo.git');
    await userEvent.click(screen.getByRole('button', { name: /Token de acceso/ }));
    await userEvent.type(screen.getByLabelText('Usuario'), 'javier');
    const field = screen.getByLabelText('Token') as HTMLInputElement;
    await userEvent.type(field, PAT);
    expect(field).toHaveAttribute('type', 'password');
    await userEvent.click(screen.getByRole('button', { name: 'Clonar' }));
    await waitFor(() => expect(onclose).toHaveBeenCalled());
    expect(cloneSpy).toHaveBeenCalledTimes(1);
    expect(cloneSpy.mock.calls[0]?.[3]).toEqual({ username: 'javier', secret: PAT });
    expect(storeSpy).not.toHaveBeenCalled();
    // El token no queda en ningún campo ni texto de la página.
    expect(field.value).toBe('');
    expect(document.body.innerHTML).not.toContain(PAT);
    // Se abrió el repo clonado en una pestaña.
    expect(app.entries.some((e) => e.kind === 'repo' && e.info.name === 'repo')).toBe(true);
  });

  it('un token inválido da AUTH, no se muestra y el campo queda vacío para reintentar', async () => {
    const { cloneSpy } = await setup();
    await fill('https://dev.azure.com/org/proj/_git/repo');
    await userEvent.click(screen.getByRole('button', { name: /Token de acceso/ }));
    await userEvent.type(screen.getByLabelText('Token'), 'invalid-token');
    await userEvent.click(screen.getByRole('button', { name: 'Clonar' }));
    expect(await screen.findByText('No se pudo autenticar')).toBeInTheDocument();
    expect(cloneSpy.mock.calls[0]?.[3]).toEqual({ username: '', secret: 'invalid-token' });
    expect((screen.getByLabelText('Token') as HTMLInputElement).value).toBe('');
    expect(document.body.innerHTML).not.toContain('invalid-token');
  });

  it('SSH no pide token y no envía credenciales', async () => {
    const { cloneSpy, onclose } = await setup();
    await fill('git@github.com:org/repo.git');
    expect(screen.queryByLabelText('Token')).not.toBeInTheDocument();
    expect(screen.getByText(/clave SSH/)).toBeInTheDocument();
    await userEvent.click(screen.getByRole('button', { name: 'Clonar' }));
    await waitFor(() => expect(onclose).toHaveBeenCalled());
    expect(cloneSpy.mock.calls[0]?.[3]).toBeNull();
  });

  it('un error genérico se muestra con su detalle', async () => {
    await setup();
    await fill('https://github.com/org/no-existe.git');
    await userEvent.click(screen.getByRole('button', { name: 'Clonar' }));
    expect(await screen.findByText('No se pudo clonar el repositorio')).toBeInTheDocument();
  });
});

describe('CloneFlow', () => {
  it('cancelar a mitad aborta, no abre nada y vuelve a reposo sin error', async () => {
    const api = mockApi({ latency: 80 });
    const opened = vi.fn();
    const flow = new CloneFlow(api, opened);
    flow.setUrl('https://github.com/org/repo.git');
    flow.setDestination('/tmp/x/repo');
    const run = flow.clone();
    await waitFor(() => expect(flow.progress?.operationId).toBeGreaterThan(0));
    await flow.cancel();
    expect(await run).toBe(false);
    expect(opened).not.toHaveBeenCalled();
    expect(flow.status).toBe('idle');
    expect(flow.error).toBeNull();
  });

  it('el progreso llega por eventos y termina con la pestaña abierta', async () => {
    const api = mockApi({ latency: 10 });
    const opened = vi.fn();
    const flow = new CloneFlow(api, opened);
    const seen: (number | null)[] = [];
    await api.listen('clone-progress', (p) => seen.push(p.percent));
    flow.setUrl('https://github.com/org/repo.git');
    flow.setDestination('/tmp/y/repo');
    expect(await flow.clone()).toBe(true);
    expect(opened).toHaveBeenCalledTimes(1);
    expect(seen).toContain(47);
    expect(seen).toContain(100);
  });

  it('dispose borra el token', () => {
    const flow = new CloneFlow(mockApi(), () => {});
    flow.secret = PAT;
    flow.dispose();
    expect(flow.secret).toBe('');
  });
});
