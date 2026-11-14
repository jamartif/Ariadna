import { cleanup, render, screen, waitFor } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { MockOptions } from '$lib/api/mockApi';
import { CloneFlow } from '$lib/stores/clone.svelte';
import { expectNoSeriousViolations } from '../../test/axe';
import { mockApi, readyApp, resetGlobals } from '../../test/helpers';
import CloneDialog from './CloneDialog.svelte';

beforeEach(resetGlobals);
afterEach(() => {
  cleanup();
  resetGlobals();
});

const PAT = 'ghp_0123456789abcdefghij';

async function setup(opts: MockOptions = {}) {
  const ctx = await readyApp(opts);
  ctx.app.newTab();
  const onclose = vi.fn();
  const cloneSpy = vi.spyOn(ctx.api, 'cloneRepository');
  const storeSpy = vi.spyOn(ctx.api, 'storeCredential');
  const testSpy = vi.spyOn(ctx.api, 'testCredential');
  render(CloneDialog, { props: { app: ctx.app, onclose } });
  return { ...ctx, onclose, cloneSpy, storeSpy, testSpy };
}

async function fill(url: string, dest = '/tmp/destino/repo') {
  await userEvent.type(screen.getByLabelText('URL del repositorio'), url);
  await userEvent.type(screen.getByLabelText('Carpeta de destino'), dest);
}

/** Abre la sección de credenciales (donde vive el selector de método). */
async function openCredentials() {
  await userEvent.click(screen.getByRole('button', { name: /Acceso al repositorio/ }));
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
    await userEvent.type(url, 'https://usuario:***@github.com/o/r.git');
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
    await openCredentials();
    await userEvent.type(screen.getByLabelText('Usuario'), 'javier');
    const field = screen.getByLabelText('Token') as HTMLInputElement;
    await userEvent.type(field, PAT);
    expect(field).toHaveAttribute('type', 'password');
    await userEvent.click(screen.getByRole('button', { name: 'Clonar' }));
    await waitFor(() => expect(onclose).toHaveBeenCalled());
    expect(cloneSpy).toHaveBeenCalledTimes(1);
    expect(cloneSpy.mock.calls[0]?.[3]).toEqual({ method: 'pat', username: 'javier', secret: PAT });
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
    await openCredentials();
    await userEvent.type(screen.getByLabelText('Token'), 'invalid-token');
    await userEvent.click(screen.getByRole('button', { name: 'Clonar' }));
    expect(await screen.findByText('No se pudo autenticar')).toBeInTheDocument();
    expect(cloneSpy.mock.calls[0]?.[3]).toEqual({ method: 'pat', username: '', secret: 'invalid-token' });
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

describe('Métodos de acceso', () => {
  it('ofrece la sesión de GitHub CLI con su estado real y clona con ella', async () => {
    const { cloneSpy, onclose } = await setup();
    await fill('https://github.com/org/private-repo.git');
    await openCredentials();
    // `gh` instalado y con sesión (por defecto del mock): la opción está lista.
    const gh = await screen.findByRole('radio', { name: /Sesión de GitHub CLI/ });
    expect(screen.getByText('Sesión lista')).toBeInTheDocument();
    await userEvent.click(gh);
    expect(gh).toHaveAttribute('aria-checked', 'true');
    // Sin token en ningún campo: solo se manda el método.
    expect(screen.queryByLabelText('Token')).not.toBeInTheDocument();
    await userEvent.click(screen.getByRole('button', { name: 'Clonar' }));
    await waitFor(() => expect(onclose).toHaveBeenCalled());
    expect(cloneSpy.mock.calls[0]?.[3]).toEqual({ method: 'gh' });
    expect(document.body.innerHTML).not.toContain('gho_');
  });

  it('sin sesión en el CLI lo dice, da el comando y no deja clonar', async () => {
    const { cloneSpy } = await setup({ auth: { gh: true, ghLogged: false } });
    await fill('https://github.com/org/private-repo.git');
    await openCredentials();
    await userEvent.click(await screen.findByRole('radio', { name: /Sesión de GitHub CLI/ }));
    expect(screen.getAllByText(/sin sesión iniciada/).length).toBeGreaterThan(0);
    expect(screen.getByText('gh auth login')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Clonar' })).toBeDisabled();
    expect(cloneSpy).not.toHaveBeenCalled();
  });

  it('con el CLI ausente la opción lo explica y sigue ofreciendo el PAT', async () => {
    await setup({ auth: { gh: false } });
    await fill('https://github.com/org/private-repo.git');
    await openCredentials();
    expect(await screen.findByText(/No se encuentra «gh»/)).toBeInTheDocument();
    await userEvent.click(screen.getByRole('radio', { name: /Token de acceso/ }));
    expect(screen.getByLabelText('Token')).toBeInTheDocument();
  });

  it('GitLab ofrece glab (y no la sesión de GitHub)', async () => {
    await setup({ auth: { glab: true, glabLogged: true } });
    await fill('https://gitlab.com/org/repo.git');
    await openCredentials();
    expect(await screen.findByRole('radio', { name: /Sesión de GitLab CLI/ })).toBeInTheDocument();
    expect(screen.queryByRole('radio', { name: /Sesión de GitHub CLI/ })).not.toBeInTheDocument();
    expect(screen.getByText('Sesión lista')).toBeInTheDocument();
  });

  it('sin client_id la opción de OAuth no aparece', async () => {
    await setup();
    await fill('https://github.com/org/private-repo.git');
    await openCredentials();
    await screen.findByRole('radio', { name: /Sesión de GitHub CLI/ });
    expect(screen.queryByRole('radio', { name: /OAuth/ })).not.toBeInTheDocument();
  });

  it('con client_id, OAuth pide código, abre el navegador y habilita el clonado al autorizar', async () => {
    const { cloneSpy, onclose, api } = await setup({ auth: { oauth: true } });
    const openSpy = vi.spyOn(api, 'openExternalUrl');
    await fill('https://github.com/org/private-repo.git');
    await openCredentials();
    await userEvent.click(await screen.findByRole('radio', { name: /OAuth/ }));
    await userEvent.click(screen.getByRole('button', { name: /Iniciar sesión en el navegador/ }));
    // El código para el usuario y la página abierta en el navegador del sistema.
    expect(await screen.findByText('ABCD-1234')).toBeInTheDocument();
    expect(openSpy).toHaveBeenCalledWith('https://github.com/login/device');
    // Un sondeo pendiente y otro autorizado habilitan el botón.
    await waitFor(() => expect(screen.getByRole('button', { name: 'Clonar' })).toBeEnabled(), { timeout: 4000 });
    expect(screen.getByText(/Autorizado/)).toBeInTheDocument();
    await userEvent.click(screen.getByRole('button', { name: 'Clonar' }));
    await waitFor(() => expect(onclose).toHaveBeenCalled());
    expect(cloneSpy.mock.calls[0]?.[3]).toEqual({ method: 'oauth', flowId: 'oauth-flow-1' });
  });

  it('«Comprobar» valida el método elegido sin clonar', async () => {
    const { testSpy, cloneSpy } = await setup();
    await fill('https://github.com/org/private-repo.git');
    await openCredentials();
    await userEvent.type(screen.getByLabelText('Token'), PAT);
    await userEvent.click(screen.getByRole('button', { name: /Comprobar/ }));
    expect(await screen.findByText('Hay acceso al repositorio.')).toBeInTheDocument();
    expect(testSpy).toHaveBeenCalledWith('https://github.com/org/private-repo.git', { method: 'pat', username: '', secret: PAT });
    expect(cloneSpy).not.toHaveBeenCalled();
  });

  it('«Comprobar» con un token inválido no dice que haya acceso', async () => {
    await setup();
    await fill('https://github.com/org/private-repo.git');
    await openCredentials();
    await userEvent.type(screen.getByLabelText('Token'), 'invalid-token');
    await userEvent.click(screen.getByRole('button', { name: /Comprobar/ }));
    expect(await screen.findByText('Sin acceso con este método.')).toBeInTheDocument();
  });

  it('el diálogo con el selector de métodos no tiene violaciones graves de accesibilidad', async () => {
    await setup({ auth: { oauth: true } });
    await fill('https://github.com/org/repo-privado.git');
    await openCredentials();
    await screen.findByRole('radio', { name: /OAuth/ });
    await expectNoSeriousViolations();
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

  it('los métodos dependen del proveedor y del estado real del CLI', async () => {
    const flow = new CloneFlow(mockApi({ auth: { gh: false, glab: true, glabLogged: true } }), () => {});
    flow.setUrl('https://gitlab.com/o/r.git');
    await waitFor(() => expect(flow.authOptions?.provider).toBe('gitlab'));
    expect(flow.methods).toEqual(['pat', 'glab']);
    expect(flow.authOptions?.cli[0]?.loggedIn).toBe(true);
    flow.setMethod('glab');
    expect(flow.cliStatus?.loggedIn).toBe(true);
    expect(flow.canClone).toBe(false); // falta la carpeta de destino
    // Al cambiar de proveedor se vuelve a consultar (GitHub aquí no tiene sesión).
    flow.setUrl('https://github.com/o/r.git');
    await waitFor(() => expect(flow.authOptions?.provider).toBe('github'));
    expect(flow.methods).toEqual(['pat', 'gh']);
    // El método anterior ya no existe para este proveedor: vuelve al PAT.
    expect(flow.method).toBe('pat');
    flow.setMethod('gh');
    expect(flow.cliStatus?.installed).toBe(false);
    expect(flow.canClone).toBe(false);
  });

  it('sin HTTPS no hay métodos ni credenciales', async () => {
    const flow = new CloneFlow(mockApi(), () => {});
    flow.setUrl('git@github.com:o/r.git');
    await flow.loadAuthOptions();
    expect(flow.wantsCredentials).toBe(false);
    expect(flow.authOptions).toBeNull();
    expect(flow.methods).toEqual(['pat']);
  });

  it('stopOauth cancela el flujo y olvida el código', async () => {
    const api = mockApi({ auth: { oauth: true } });
    const cancelSpy = vi.spyOn(api, 'oauthDeviceCancel');
    const flow = new CloneFlow(api, () => {});
    flow.setUrl('https://github.com/o/r.git');
    await flow.loadAuthOptions();
    flow.setMethod('oauth');
    await flow.startOauth();
    expect(flow.oauth?.userCode).toBe('ABCD-1234');
    await flow.stopOauth();
    expect(flow.oauth).toBeNull();
    expect(flow.oauthState).toBe('idle');
    expect(cancelSpy).toHaveBeenCalledWith('oauth-flow-1');
    flow.dispose();
  });
});
