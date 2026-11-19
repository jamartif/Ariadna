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

const PULL = {
  name: '7/head',
  oid: '009cbe6000000000000000000000000000000000',
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
  async function setup() {
    const ctx = await loadedTab('tab-ariadna');
    const repo = ctx.api.__repos.get('tab-ariadna')!;
    repo.pullRefs = [PULL];
    repo.worktrees = [
      { path: '/repos/ariadna', head: 'aaaaaaa', branch: 'main', detached: false, isMain: true },
      { path: 'C:/c/Users/javie/dev/ariadna-pr', head: '009cbe6000000000000000000000000000000000', branch: null, detached: true, isMain: false }
    ];
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
    expect(row).toHaveTextContent('7/head');
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

  it('el filtro del riel también reduce estos grupos', async () => {
    await setup();
    await screen.findByTestId('rail-pull-ref');
    const input = screen.getByLabelText('Buscar rama') as HTMLInputElement;
    await userEvent.clear(input);
    await userEvent.type(input, 'no-existe-esto');
    await waitFor(() => expect(screen.queryByTestId('rail-pull-ref')).not.toBeInTheDocument());
  });
});
