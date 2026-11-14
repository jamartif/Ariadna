import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { branchesOf } from './branches.svelte';
import { toasts } from './toasts.svelte';
import { loadedTab, resetGlobals, waitFor } from '../../test/helpers';

beforeEach(resetGlobals);
afterEach(() => {
  resetGlobals();
  vi.restoreAllMocks();
});

async function setup(opts = {}) {
  const ctx = await loadedTab('tab-ariadna', opts);
  return { ...ctx, b: branchesOf(ctx.tab) };
}

describe('BranchesController: acciones del menú contextual', () => {
  it('pushBranch publica una rama sin origen y le fija el seguimiento', async () => {
    const { api, tab, b } = await setup();
    const spy = vi.spyOn(api, 'pushBranch');
    // `feature/command-palette` no tiene upstream: Rust la publica con `--set-upstream`.
    expect(await b.pushBranch('feature/command-palette')).toBe(true);
    expect(spy).toHaveBeenCalledWith(tab.id, 'feature/command-palette', true);
    await waitFor(() => expect(b.locals.find((x) => x.name === 'feature/command-palette')?.upstream).toBe('origin/feature/command-palette'));
    expect(toasts.items.some((t) => t.kind === 'success' && /subida a «origin»/.test(t.title))).toBe(true);
  });

  it('pushBranch no vuelve a fijar el seguimiento si la rama ya lo tiene', async () => {
    const { api, tab, b } = await setup();
    const spy = vi.spyOn(api, 'pushBranch');
    expect(await b.pushBranch('main')).toBe(true);
    expect(spy).toHaveBeenCalledWith(tab.id, 'main', false);
  });

  it('pushBranch con una rama que no existe no llama a Rust', async () => {
    const { api, b } = await setup();
    const spy = vi.spyOn(api, 'pushBranch');
    expect(await b.pushBranch('fantasma')).toBe(false);
    expect(spy).not.toHaveBeenCalled();
  });

  it('rebase rebasa la rama actual sobre la indicada', async () => {
    const { api, tab, b } = await setup();
    const spy = vi.spyOn(api, 'rebaseOnto');
    expect(await b.rebase('main')).toBe(true);
    expect(spy).toHaveBeenCalledWith(tab.id, 'main');
    expect(toasts.items.some((t) => t.kind === 'success' && /rebasada sobre «main»/.test(t.title))).toBe(true);
    expect(b.busy).toBeNull();
  });

  it('rebase con una rama que ya no existe deja un aviso de error', async () => {
    const { api, tab, b } = await setup();
    vi.spyOn(api, 'rebaseOnto').mockRejectedValue({ code: 'INVALID_REFERENCE', message: 'No existe la referencia fantasma.' });
    expect(await b.rebase('fantasma')).toBe(false);
    expect(toasts.items.some((t) => t.kind === 'error' && /No se pudo rebasar/.test(t.title))).toBe(true);
    expect(b.busy).toBeNull();
    expect(b.locals.find((x) => x.isHead)?.name).toBe('rust-migration');
  });

  it('rebase cancelado en la confirmación nativa no avisa de nada', async () => {
    const { api, tab } = await setup();
    vi.spyOn(api, 'rebaseOnto').mockResolvedValue({ done: false, result: 'upToDate', state: null });
    const b = branchesOf(tab);
    expect(await b.rebase('main')).toBe(false);
    expect(toasts.items).toHaveLength(0);
  });

  it('removeRemote borra la rama del remoto y limpia el seguimiento', async () => {
    const { b } = await setup();
    expect(b.remotes.some((x) => x.name === 'origin/main')).toBe(true);
    expect(await b.removeRemote('origin', 'main')).toBe(true);
    await waitFor(() => expect(b.remotes.some((x) => x.name === 'origin/main')).toBe(false));
    expect(b.locals.find((x) => x.name === 'main')?.upstream).toBeNull();
    expect(toasts.items.some((t) => t.kind === 'success' && /origin\/main/.test(t.title))).toBe(true);
  });

  it('removeRemote respeta la cancelación de Rust y los errores', async () => {
    const { b } = await setup({ confirm: () => false });
    expect(await b.removeRemote('origin', 'main')).toBe(false);
    expect(b.remotes.some((x) => x.name === 'origin/main')).toBe(true);
    await expect(b.tab.api.deleteRemoteBranch(b.tab.id, 'origin', 'fantasma')).rejects.toMatchObject({
      code: 'INVALID_REFERENCE'
    });
  });

  it('compare carga el diff entre dos ramas y clearDiff lo quita', async () => {
    const { api, tab, b } = await setup();
    const spy = vi.spyOn(api, 'branchDiffAgainst');
    await b.compare('rust-migration', 'main');
    expect(spy).toHaveBeenCalledWith(tab.id, 'rust-migration', 'main');
    expect(b.diff?.base).toBe('rust-migration');
    expect(b.diff?.head).toBe('main');
    expect(b.diff?.files.length).toBeGreaterThan(0);
    expect(b.diffBusy).toBe(false);
    b.clearDiff();
    expect(b.diff).toBeNull();
    expect(b.diffError).toBeNull();
  });

  it('compare con la misma rama no pide nada y vacía el diff', async () => {
    const { api, b } = await setup();
    const spy = vi.spyOn(api, 'branchDiffAgainst');
    await b.compare('main', 'main');
    expect(spy).not.toHaveBeenCalled();
    expect(b.diff).toBeNull();
  });

  it('compare deja el error a la vista sin romper el detalle', async () => {
    const { b } = await setup();
    await b.compare('main', 'fantasma');
    expect(b.diff).toBeNull();
    expect(b.diffError).toMatchObject({ code: 'INVALID_REFERENCE' });
    expect(b.diffBusy).toBe(false);
    // La siguiente comparación buena limpia el error.
    await b.compare('rust-migration', 'main');
    expect(b.diffError).toBeNull();
    expect(b.diff).not.toBeNull();
  });
});
