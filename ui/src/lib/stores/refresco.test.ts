import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { prOf } from './pullRequests.svelte';
import { historyOf } from './history.svelte';
import { resetGlobals, readyApp } from '../../test/helpers';
import { prTab } from '../../test/prHelpers';

beforeEach(resetGlobals);
afterEach(() => {
  resetGlobals();
  vi.restoreAllMocks();
});

const completeOptions = {
  strategy: 'squash' as const,
  deleteSourceBranch: true,
  completeWorkItems: false,
  autoComplete: false,
  commitMessage: null
};

/**
 * Al mergear una PR el repo cambia: la PR pasa a completada, la rama puede desaparecer y el commit
 * de merge entra en el historial. Sin refresco, la interfaz sigue enseñando el estado anterior hasta
 * que el usuario fuerza. Y si el merge falla, no se refresca nada a medias.
 */
describe('refresco al completar una PR', () => {
  it('vuelve a pedir estado, historial y lista de PRs tras un merge correcto', async () => {
    const { pr, tab, api } = await prTab({ confirm: () => true });
    const history = historyOf(tab);
    await history.reload();
    await pr.loadList();
    pr.select(101);
    await vi.waitFor(() => expect(pr.detail?.id).toBe(101));

    const refresh = vi.spyOn(api, 'refresh');
    const log = vi.spyOn(api, 'logPage');
    const list = vi.spyOn(api, 'listPullRequests');

    expect(await pr.complete(completeOptions)).toBe(true);

    await vi.waitFor(() => expect(refresh).toHaveBeenCalledWith('tab-ariadna'));
    await vi.waitFor(() => expect(log).toHaveBeenCalled());
    await vi.waitFor(() => expect(list.mock.calls.length).toBeGreaterThan(0), { timeout: 4000 });
    await vi.waitFor(() => expect(pr.detail?.state).toBe('completed'));
  });

  it('si el merge falla no se refresca nada y el estado anterior se queda', async () => {
    const { pr, tab, api } = await prTab({ confirm: () => true });
    const history = historyOf(tab);
    await history.reload();
    pr.select(105);
    await vi.waitFor(() => expect(pr.detail?.id).toBe(105));
    const before = tab.revision;

    const refresh = vi.spyOn(api, 'refresh');
    const log = vi.spyOn(api, 'logPage');

    expect(await pr.complete({ ...completeOptions, strategy: 'merge' })).toBe(false);
    expect(pr.actionError?.code).toBe('POLICY');
    expect(refresh).not.toHaveBeenCalled();
    expect(log).not.toHaveBeenCalled();
    expect(tab.revision).toBe(before);
    expect(pr.detail?.state).toBe('active');
  });
});

/**
 * Cambiar de pestaña de repositorio tiene que recargar lo suyo y no lo que había en memoria. Si
 * vuelves a la misma pestaña enseguida, no se repiten las peticiones.
 */
describe('refresco al activar otra pestaña de repo', () => {
  it('recarga ramas, historial, cambios y PRs del repo activado', async () => {
    const { app, api } = await readyApp();
    const refresh = vi.spyOn(api, 'refresh');
    const log = vi.spyOn(api, 'logPage');
    const prs = vi.spyOn(api, 'listPullRequests');

    app.activate('tab-legacy');
    await vi.waitFor(() => expect(refresh).toHaveBeenCalledWith('tab-legacy'));
    await vi.waitFor(() => expect(log.mock.calls.some((c) => c[0] === 'tab-legacy')).toBe(true));

    // `tab-ariadna` tiene remoto de GitHub: al activarlo se piden también sus PRs.
    app.activate('tab-ariadna');
    await vi.waitFor(() => expect(refresh).toHaveBeenCalledWith('tab-ariadna'));
    await vi.waitFor(() => expect(log.mock.calls.some((c) => c[0] === 'tab-ariadna')).toBe(true));
    await vi.waitFor(() => expect(prs.mock.calls.some((c) => c[0] === 'tab-ariadna')).toBe(true), {
      timeout: 4000
    });
  });

  it('volver enseguida a la misma pestaña no duplica peticiones', async () => {
    const { app, api } = await readyApp();
    const refresh = vi.spyOn(api, 'refresh');
    app.activate('tab-legacy');
    await vi.waitFor(() => expect(refresh).toHaveBeenCalled());
    const calls = refresh.mock.calls.length;

    app.activate('tab-ariadna');
    await vi.waitFor(() => expect(refresh.mock.calls.some((c) => c[0] === 'tab-ariadna')).toBe(true));
    app.activate('tab-legacy');
    // Reciente: no se vuelve a pedir el estado del repo al que se vuelve.
    expect(refresh.mock.calls.length).toBe(calls + 1);
    expect(prOf).toBeTypeOf('function');
  });
});
