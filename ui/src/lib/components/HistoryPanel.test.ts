import { render, screen, waitFor, within } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { TabState } from '$lib/stores/tab.svelte';
import { historyOf } from '$lib/stores/history.svelte';
import { loadedTab, mockApi, resetGlobals } from '../../test/helpers';
import HistoryDetail from './HistoryDetail.svelte';
import HistoryPanel from './HistoryPanel.svelte';

beforeEach(resetGlobals);
afterEach(resetGlobals);

async function setup(tabId = 'tab-ariadna') {
  const { tab, api } = await loadedTab(tabId);
  render(HistoryPanel, { props: { tab } });
  render(HistoryDetail, { props: { tab } });
  return { tab, api };
}

describe('Historial', () => {
  it('lista los commits con autor, fecha relativa, hash corto y refs', async () => {
    const { tab } = await setup();
    const list = await screen.findByRole('listbox', { name: 'Lista de commits' });
    await waitFor(() => expect(within(list).getAllByRole('option').length).toBeGreaterThan(3));
    const first = within(list).getAllByRole('option')[0]!;
    const c = historyOf(tab).commits[0]!;
    expect(first).toHaveTextContent(c.subject);
    expect(first).toHaveTextContent(c.authorName);
    expect(first).toHaveTextContent(c.sha.slice(0, 7));
    expect(first).toHaveTextContent('main');
    expect(first.querySelector('time')).not.toBeNull();
  });

  it('seleccionar un commit muestra sus archivos y el diff', async () => {
    const { tab } = await setup();
    const list = await screen.findByRole('listbox', { name: 'Lista de commits' });
    await waitFor(() => expect(within(list).getAllByRole('option').length).toBeGreaterThan(2));
    const second = within(list).getAllByRole('option')[1]!;
    await userEvent.click(second);
    const h = historyOf(tab);
    expect(h.selected?.sha).toBe(h.commits[1]?.sha);
    expect(second).toHaveAttribute('aria-selected', 'true');
    const files = await screen.findByRole('listbox', { name: 'Archivos del commit' });
    expect(within(files).getAllByRole('option').length).toBeGreaterThan(0);
    // El diff del archivo seleccionado se pinta (líneas añadidas/borradas accesibles).
    expect(await screen.findByRole('region', { name: /Código de/ })).toBeInTheDocument();
    expect(screen.getByRole('heading', { level: 2, name: h.selected!.subject })).toBeInTheDocument();
  });

  it('las flechas mueven la selección', async () => {
    const { tab } = await setup();
    const list = await screen.findByRole('listbox', { name: 'Lista de commits' });
    await waitFor(() => expect(historyOf(tab).selected).not.toBeNull());
    list.focus();
    await userEvent.keyboard('{ArrowDown}{ArrowDown}');
    const h = historyOf(tab);
    await waitFor(() => expect(h.selected?.sha).toBe(h.commits[2]?.sha));
    await userEvent.keyboard('{ArrowUp}');
    await waitFor(() => expect(h.selected?.sha).toBe(h.commits[1]?.sha));
  });

  it('buscar filtra y avisa cuando no hay resultados', async () => {
    const { tab } = await setup();
    await waitFor(() => expect(historyOf(tab).commits.length).toBeGreaterThan(0));
    await userEvent.type(screen.getByRole('searchbox', { name: 'Buscar en el historial' }), 'zzz-no-existe');
    expect(await screen.findByText('Sin resultados')).toBeInTheDocument();
    expect(screen.getByText(/Ningún commit coincide con «zzz-no-existe»/)).toBeInTheDocument();
  });

  it('con un historial enorme solo se pintan unas pocas filas (virtualizado) y se pagina', async () => {
    const api = mockApi();
    const res = await api.openRepository('/home/javie/proyectos/monorepo-gigante');
    const tab = new TabState(api, res.tab);
    await tab.attach();
    await tab.load();
    render(HistoryPanel, { props: { tab } });
    const list = await screen.findByRole('listbox', { name: 'Lista de commits' });
    await waitFor(() => expect(historyOf(tab).commits.length).toBe(100));
    expect(within(list).getAllByRole('option').length).toBeLessThan(40);
    expect(list.querySelectorAll('svg.graph').length).toBeGreaterThan(0);
    expect(screen.getByText(/100\+? commits cargados/)).toBeInTheDocument();
  });

  it('un repo sin commits muestra el estado vacío', async () => {
    const { tab, api } = await loadedTab('tab-ariadna');
    api.logPage = async () => [];
    render(HistoryPanel, { props: { tab } });
    expect(await screen.findByText('Todavía no hay commits')).toBeInTheDocument();
  });
});
