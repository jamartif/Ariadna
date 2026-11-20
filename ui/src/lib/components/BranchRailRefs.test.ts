import { render, screen, waitFor, within } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { loadedTab, resetGlobals } from '../../test/helpers';
import BranchRail from './BranchRail.svelte';
import Toasts from './Toasts.svelte';

beforeEach(resetGlobals);
afterEach(() => {
  resetGlobals();
  vi.restoreAllMocks();
});

/** El motor manda la referencia completa: así un PR de GitHub y un MR de GitLab con el mismo número
 *  no colisionan (antes las dos llegaban como `7/head` y la clave del `#each` se repetía). */
const PULL = {
  name: 'refs/pull/7/head',
  oid: '009cbe6000000000000000000000000000000000',
  isHead: false,
  upstream: null,
  ahead: 0,
  behind: 0
};
const MR = {
  name: 'refs/merge-requests/7/head',
  oid: '009cbe6000000000000000000000000000000001',
  isHead: false,
  upstream: null,
  ahead: 0,
  behind: 0
};

/**
 * El riel pinta las refs que no viven en el snapshot diario: refs de pull request y los **otros**
 * worktrees. Antes no había ni grupo ni lectura, así que Javier no las veía.
 */
describe('riel · pull requests y worktrees', () => {
  async function setup(opts: { pullRefs?: typeof PULL[]; fallaExtra?: boolean } = {}) {
    const ctx = await loadedTab('tab-ariadna');
    const repo = ctx.api.__repos.get('tab-ariadna')!;
    repo.pullRefs = opts.pullRefs ?? [PULL];
    repo.worktrees = [
      { path: '/repos/ariadna', head: 'aaaaaaa', branch: 'main', detached: false, isMain: true },
      { path: 'C:/c/Users/javie/dev/ariadna-pr', head: '009cbe6000000000000000000000000000000000', branch: null, detached: true, isMain: false }
    ];
    if (opts.fallaExtra) {
      ctx.api.extraRefs = async () => {
        throw new Error('boom');
      };
    }
    render(BranchRail, { props: { tab: ctx.tab, collapsed: false, ontoggle: vi.fn(), oninspect: vi.fn(), onrepo: vi.fn() } });
    render(Toasts);
    return ctx;
  }

  it('muestra el grupo de pull requests con su entrada y la referencia completa', async () => {
    await setup();
    const group = await screen.findByTestId('rail-pull');
    expect(within(group).getByText('Refs de PR')).toBeInTheDocument();
    const row = await screen.findByTestId('rail-pull-ref');
    expect(row).toHaveAttribute('title', 'refs/pull/7/head');
    expect(row).toHaveTextContent('#7/head');
  });

  it('distingue un PR de GitHub de un MR de GitLab con el mismo número', async () => {
    await setup({ pullRefs: [PULL, MR] });
    const filas = await screen.findAllByTestId('rail-pull-ref');
    expect(filas).toHaveLength(2);
    expect(filas.map((f) => f.getAttribute('title'))).toEqual([
      'refs/pull/7/head',
      'refs/merge-requests/7/head'
    ]);
    expect(filas[0]).toHaveTextContent('#7/head');
    expect(filas[1]).toHaveTextContent('!7/head');
  });

  it('muestra los otros worktrees (sin el principal) y marca el detached', async () => {
    await setup();
    const group = await screen.findByTestId('rail-worktrees');
    expect(within(group).getByText('Otros worktrees')).toBeInTheDocument();
    const rows = await screen.findAllByTestId('rail-worktree');
    expect(rows).toHaveLength(1);
    expect(rows[0]).toHaveAttribute('title', 'C:/c/Users/javie/dev/ariadna-pr');
    expect(rows[0]).toHaveTextContent('sin rama');
  });

  it('el filtro aplica la misma regla a los dos grupos y los deja con su aviso', async () => {
    await setup();
    await screen.findByTestId('rail-pull-ref');
    const input = screen.getByLabelText('Buscar rama') as HTMLInputElement;
    await userEvent.clear(input);
    await userEvent.type(input, 'no-existe-esto');

    await waitFor(() => expect(screen.queryByTestId('rail-pull-ref')).not.toBeInTheDocument());
    await waitFor(() => expect(screen.queryByTestId('rail-worktree')).not.toBeInTheDocument());
    // Los grupos siguen ahí con su aviso: antes los worktrees desaparecían y los PRs decían «sin
    // coincidencias» con una regla distinta.
    expect(screen.getByTestId('rail-pull')).toHaveTextContent(/Ninguna rama coincide/);
    expect(screen.getByTestId('rail-worktrees')).toHaveTextContent(/Ninguna rama coincide/);
  });

  it('el filtro encuentra un worktree por su ruta, igual que una rama por su nombre', async () => {
    await setup();
    await screen.findByTestId('rail-worktree');
    const input = screen.getByLabelText('Buscar rama') as HTMLInputElement;
    await userEvent.clear(input);
    await userEvent.type(input, 'ariadna-pr');
    await waitFor(() => expect(screen.getByTestId('rail-worktree')).toBeInTheDocument());
  });

  it('si extraRefs falla, se vacían los grupos y el fallo queda en la consola', async () => {
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => {});
    await setup({ fallaExtra: true });
    await waitFor(() => expect(warn).toHaveBeenCalled());
    expect(String(warn.mock.calls[0]?.[0])).toContain('extraRefs');
    expect(screen.queryByTestId('rail-pull')).not.toBeInTheDocument();
    expect(screen.queryByTestId('rail-worktrees')).not.toBeInTheDocument();
  });
});
