import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { remotesOf } from './remotes.svelte';
import { toasts } from './toasts.svelte';
import { loadedTab, resetGlobals, waitFor } from '../../test/helpers';

beforeEach(resetGlobals);
afterEach(resetGlobals);

async function setup(opts = {}) {
  const ctx = await loadedTab('tab-ariadna', opts);
  const r = remotesOf(ctx.tab);
  await r.load();
  return { ...ctx, r };
}

describe('RemotesController', () => {
  it('lista los remotos con el proveedor que calcula Rust', async () => {
    const { r } = await setup();
    expect(r.views).toHaveLength(1);
    expect(r.views[0]).toMatchObject({ name: 'origin', provider: 'github', providerLabel: 'GitHub', host: 'github.com', canOpen: true, canCreatePullRequest: true, ssh: true });
  });

  it('abre repo, rama y nueva PR por Rust con la rama sin prefijo del remoto', async () => {
    const { r, api } = await setup();
    expect(await r.open('origin', 'repo')).toBe(true);
    const link = r.linkFor('origin/feature/payments', true)!;
    expect(link.branch).toBe('feature/payments');
    await r.open(link.remote.name, 'pull-request', link.branch);
    await r.open('origin', 'branch', 'rust-migration');
    expect(api.__openedUrls()).toEqual([
      'https://github.com/javie/ariadna',
      'https://github.com/javie/ariadna/pull/new/feature/payments',
      'https://github.com/javie/ariadna/tree/rust-migration'
    ]);
  });

  it('linkFor: una rama local usa el remoto de su upstream y si no, origin', async () => {
    const { r, tab } = await setup();
    const head = tab.snapshot!.branches.find((b) => b.isHead)!;
    expect(r.linkFor(head.name, false)).toMatchObject({ remote: { name: 'origin' }, branch: head.name });
    expect(r.linkFor('origin/main', true)).toMatchObject({ remote: { name: 'origin' }, branch: 'main' });
    expect(r.linkFor('otro/main', true)).toBeNull();
  });

  it('un remoto sin proveedor conocido abre el repo pero no ofrece pull request', async () => {
    const { r, api, tab } = await setup();
    expect(await r.add('gitea', 'https://git.acme.com/team/app.git')).toBe(true);
    const gitea = r.views.find((v) => v.name === 'gitea')!;
    expect(gitea).toMatchObject({ provider: 'other', host: 'git.acme.com', canOpen: true, canCreatePullRequest: false });
    expect(await r.open('gitea', 'repo')).toBe(true);
    expect(api.__openedUrls()).toEqual(['https://git.acme.com/team/app']);
    await expect(api.openRemoteUrl(tab.id, 'gitea', 'pull-request', 'x')).rejects.toMatchObject({ code: 'POLICY' });
  });

  it('un remoto local o sin dueño no tiene página web', async () => {
    const { r, api, tab } = await setup();
    await r.add('local', '/srv/git/app.git');
    expect(r.views.find((v) => v.name === 'local')).toMatchObject({ canOpen: false, canCreatePullRequest: false });
    await expect(api.openRemoteUrl(tab.id, 'local', 'repo', null)).rejects.toMatchObject({ code: 'POLICY' });
  });

  it('el enlace rechazado por Rust sale como aviso, no como excepción', async () => {
    const { r, tab } = await setup();
    expect(await r.open('no-existe', 'repo')).toBe(false);
    expect(toasts.items.some((x) => x.kind === 'error')).toBe(true);
    expect(tab.api).toBeDefined();
  });

  it('añadir: Rust confirma; cancelar no cambia nada y aceptar publica el remoto nuevo', async () => {
    let accept = false;
    const { r, tab } = await setup({ confirm: () => accept });
    expect(await r.add('copia', 'https://gitlab.com/g/copia.git')).toBe(false);
    expect(r.views.map((v) => v.name)).toEqual(['origin']);
    accept = true;
    expect(await r.add('copia', 'https://gitlab.com/g/copia.git')).toBe(true);
    expect(r.views.map((v) => v.name)).toEqual(['origin', 'copia']);
    expect(r.views[1]).toMatchObject({ provider: 'gitlab', canCreatePullRequest: true });
    await waitFor(() => expect(tab.snapshot!.remotes.map((x) => x.name)).toEqual(['origin', 'copia']));
  });

  it('añadir rechaza contraseñas y transportes raros sin tocar nada', async () => {
    const { r } = await setup();
    for (const url of ['https://u:secreto@github.com/a/b.git', 'ext::sh -c x', '-oProxyCommand=x', '']) {
      expect(await r.add('nuevo', url)).toBe(false);
    }
    expect(r.views).toHaveLength(1);
    expect(toasts.items.filter((x) => x.kind === 'error').length).toBeGreaterThanOrEqual(4);
  });

  it('renombrar actualiza ramas de seguimiento y upstream; quitar las borra', async () => {
    const { r, tab } = await setup();
    expect(await r.rename('origin', 'principal')).toBe(true);
    await waitFor(() => expect(tab.snapshot!.remotes[0]!.name).toBe('principal'));
    expect(tab.snapshot!.remoteBranches.every((b) => b.name.startsWith('principal/'))).toBe(true);
    expect(tab.snapshot!.branches.find((b) => b.isHead)!.upstream).toBe('principal/rust-migration');
    expect(await r.remove('principal')).toBe(true);
    await waitFor(() => expect(tab.snapshot!.remotes).toHaveLength(0));
    expect(tab.snapshot!.remoteBranches).toHaveLength(0);
    expect(tab.snapshot!.branches.every((b) => b.upstream === null)).toBe(true);
    expect(r.views).toHaveLength(0);
  });

  it('fetch, pull y push de la rama actual', async () => {
    const { r, api } = await setup();
    expect(await r.fetch('origin')).toBe(true);
    expect(await r.pull('origin')).toBe(true);
    expect(api.__lastPullRebase()).not.toBeNull();
    expect(await r.push('origin')).toBe(true);
    expect(toasts.items.some((x) => /actualizado|integrados|al día/.test(x.title))).toBe(true);
  });
});
