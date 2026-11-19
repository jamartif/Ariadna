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

/**
 * Criterio del historial completo: lo que lista el riel tiene que coincidir con
 * `git for-each-ref refs/heads refs/remotes refs/tags`. Las etiquetas también son referencias del
 * repo y tienen que aparecer (antes solo se listaban ramas locales y de seguimiento).
 */
describe('riel · etiquetas', () => {
  it('lista las etiquetas del repo junto a las ramas', async () => {
    const { tab, api } = await loadedTab('tab-ariadna');
    const tags = await api.listTags('tab-ariadna');
    expect(tags.length).toBeGreaterThan(0);

    render(BranchRail, { props: { tab, collapsed: false, ontoggle: vi.fn(), oninspect: vi.fn(), onrepo: vi.fn() } });
    render(Toasts);

    const group = await screen.findByTestId('rail-tags');
    expect(within(group).getByText('Etiquetas')).toBeInTheDocument();
    const rows = await screen.findAllByTestId('rail-tag');
    expect(rows.map((r) => r.getAttribute('title'))).toEqual(tags.map((g) => g.name));
    // Y siguen estando las ramas: el riel es la lista completa de referencias.
    expect(screen.getByTestId('rail-tree')).toHaveTextContent('Remotas');
    expect(screen.getByTestId('rail-tree')).toHaveTextContent('Locales');
  });

  it('el filtro también busca por nombre de etiqueta', async () => {
    const { tab, api } = await loadedTab('tab-ariadna');
    const first = (await api.listTags('tab-ariadna'))[0]!;
    render(BranchRail, { props: { tab, collapsed: false, ontoggle: vi.fn(), oninspect: vi.fn(), onrepo: vi.fn() } });
    render(Toasts);
    await screen.findAllByTestId('rail-tag');

    const input = screen.getByLabelText('Buscar rama') as HTMLInputElement;
    await userEvent.clear(input);
    await userEvent.type(input, first.name);

    await waitFor(() => {
      const rows = screen.queryAllByTestId('rail-tag');
      expect(rows.map((r) => r.getAttribute('title'))).toEqual([first.name]);
    });
  });
});
