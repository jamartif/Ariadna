import { cleanup, render, screen, waitFor, within } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { settingsUi } from '$lib/stores/settingsPanel.svelte';
import { resetGlobals } from '../../test/helpers';
import { prTab } from '../../test/prHelpers';
import PullRequestsPanel from './PullRequestsPanel.svelte';

beforeEach(resetGlobals);
afterEach(() => {
  cleanup();
  vi.useRealTimers();
  vi.restoreAllMocks();
  settingsUi.close();
  resetGlobals();
});

async function setup(opts: Parameters<typeof prTab>[0] = {}) {
  const ctx = await prTab(opts);
  render(PullRequestsPanel, { props: { tab: ctx.tab } });
  return ctx;
}

const row = (title: RegExp) => screen.getByRole('button', { name: title });
const rows = () => within(screen.getByRole('list', { name: 'Lista de pull requests' })).getAllByRole('button');

describe('Panel de pull requests: lista', () => {
  it('lista las abiertas con estado, borrador, conflictos, comprobaciones y revisores', async () => {
    await setup();
    expect(await screen.findByRole('button', { name: /Añade validación de formularios/ })).toBeInTheDocument();
    expect(rows()).toHaveLength(3);
    // Orden: la más reciente primero.
    expect(rows()[0]).toHaveTextContent('#105');
    expect(row(/Corrige el cierre de sesión/)).toHaveTextContent('Borrador');
    expect(row(/Reescribe el módulo de facturación/)).toHaveTextContent('Conflictos');
    const r101 = row(/Añade validación/);
    expect(r101).toHaveTextContent('Abierta');
    expect(r101).toHaveTextContent('feature/pr-101');
    expect(r101).toHaveTextContent('main');
    // Los votos y las comprobaciones también son texto para lectores de pantalla.
    expect(within(r101).getByText('Beto: Aprobar')).toBeInTheDocument();
    expect(within(r101).getByText('Comprobaciones — correctas: 2, fallidas: 0, pendientes: 1')).toBeInTheDocument();
    expect(screen.getByText(/GitHub · github.com\/acme\/app/)).toBeInTheDocument();
  });

  it('filtra por «creadas por mí», «donde reviso» y «cerradas»', async () => {
    await setup();
    await screen.findByRole('button', { name: /Añade validación/ });
    const user = userEvent.setup();

    await user.click(screen.getByRole('button', { name: 'Creadas por mí' }));
    await waitFor(() => expect(rows()).toHaveLength(1));
    expect(rows()[0]).toHaveTextContent('Corrige el cierre de sesión');
    expect(screen.getByRole('button', { name: 'Creadas por mí' })).toHaveAttribute('aria-pressed', 'true');

    await user.click(screen.getByRole('button', { name: 'Donde reviso' }));
    await waitFor(() => expect(rows()[0]).toHaveTextContent('Añade validación'));
    expect(rows()).toHaveLength(1);

    await user.click(screen.getByRole('button', { name: 'Cerradas' }));
    await waitFor(() => expect(rows()).toHaveLength(2));
    expect(rows()[0]).toHaveTextContent('Abandonada');
    expect(rows()[1]).toHaveTextContent('Completada');
  });

  it('busca dentro de la página y avisa si nada coincide', async () => {
    await setup();
    await screen.findByRole('button', { name: /Añade validación/ });
    const user = userEvent.setup();
    await user.type(screen.getByRole('searchbox', { name: /Buscar/ }), 'facturación');
    expect(rows()).toHaveLength(1);
    await user.clear(screen.getByRole('searchbox', { name: /Buscar/ }));
    await user.type(screen.getByRole('searchbox', { name: /Buscar/ }), 'zzz');
    expect(screen.getByRole('status')).toHaveTextContent('Ninguna pull request de esta página coincide con «zzz».');
  });

  it('seleccionar una fila la marca como actual', async () => {
    const { pr } = await setup();
    const user = userEvent.setup();
    await user.click(await screen.findByRole('button', { name: /Añade validación/ }));
    expect(pr.selectedId).toBe(101);
    expect(row(/Añade validación/)).toHaveAttribute('aria-current', 'true');
    expect(row(/Corrige el cierre/)).not.toHaveAttribute('aria-current');
  });

  it('las flechas mueven el foco entre filas', async () => {
    await setup();
    await screen.findByRole('button', { name: /Añade validación/ });
    const user = userEvent.setup();
    rows()[0]?.focus();
    await user.keyboard('{ArrowDown}');
    expect(rows()[1]).toHaveFocus();
    await user.keyboard('{End}');
    expect(rows()[2]).toHaveFocus();
    await user.keyboard('{ArrowUp}{Home}');
    expect(rows()[0]).toHaveFocus();
  });

  it('pagina con Anterior / Siguiente', async () => {
    const { api } = await setup();
    await screen.findByRole('button', { name: /Añade validación/ });
    const spy = vi.spyOn(api, 'listPullRequests').mockImplementation(async (_t, _f, n) => ({ items: [], page: n, hasMore: n < 2 }));
    const user = userEvent.setup();
    expect(screen.getByRole('button', { name: 'Página anterior' })).toBeDisabled();
    // Fuerza una recarga de la página 1 para que el mock con «hay más» se aplique.
    await user.click(screen.getByRole('button', { name: 'Actualizar' }));
    await waitFor(() => expect(screen.getByRole('button', { name: 'Página siguiente' })).toBeEnabled());
    await user.click(screen.getByRole('button', { name: 'Página siguiente' }));
    await waitFor(() => expect(screen.getByText('Página 2')).toBeInTheDocument());
    expect(spy).toHaveBeenLastCalledWith('tab-ariadna', 'open', 2);
    expect(screen.getByRole('button', { name: 'Página siguiente' })).toBeDisabled();
    await user.click(screen.getByRole('button', { name: 'Página anterior' }));
    await waitFor(() => expect(screen.getByText('Página 1')).toBeInTheDocument());
  });

  it('mientras carga muestra un estado ocupado', async () => {
    const ctx = await prTab();
    vi.spyOn(ctx.api, 'listPullRequests').mockReturnValue(new Promise(() => {}));
    render(PullRequestsPanel, { props: { tab: ctx.tab } });
    const busy = await screen.findByRole('status', { name: 'Cargando pull requests' });
    expect(busy).toHaveAttribute('aria-busy', 'true');
  });
});

describe('Panel de pull requests: sin soporte, vacío y errores tipados', () => {
  it('un remoto sin soporte (GitLab) lo explica y ofrece abrir el navegador', async () => {
    const { api } = await setup({ url: 'https://gitlab.com/g/r.git' });
    expect(await screen.findByRole('heading', { name: 'Este remoto no admite pull requests dentro de Ariadna' })).toBeInTheDocument();
    expect(screen.getByText(/se abren en el navegador/)).toBeInTheDocument();
    expect(screen.queryByRole('searchbox')).toBeNull();
    const open = vi.spyOn(api, 'openRemoteUrl');
    await userEvent.click(await screen.findByRole('button', { name: 'Abrir el repositorio en el navegador' }));
    expect(open).toHaveBeenCalledWith('tab-ariadna', 'origin', 'repo', null);
  });

  it('vacío: explica cada filtro y ofrece crear', async () => {
    await setup({ scenario: 'pr-empty' });
    expect(await screen.findByRole('heading', { name: 'No hay pull requests' })).toBeInTheDocument();
    expect(screen.getByText('No hay pull requests abiertas en este repositorio.')).toBeInTheDocument();
    await userEvent.click(screen.getByRole('button', { name: 'Donde reviso' }));
    expect(await screen.findByText('No eres revisor de ninguna pull request abierta.')).toBeInTheDocument();
  });

  it('AUTH: muestra el permiso que falta y lleva a la ayuda del PAT en Ajustes › Cuentas', async () => {
    await setup({ scenario: 'pr-auth' });
    const alert = await screen.findByRole('alert');
    expect(alert).toHaveTextContent('Falta autorización');
    expect(alert).toHaveTextContent('Permiso que falta en tu token: pull_requests=read');
    await userEvent.click(within(alert).getByRole('button', { name: /Ayuda del token/ }));
    expect(settingsUi.isOpen).toBe(true);
    expect(settingsUi.section).toBe('accounts');
  });

  it('RATE_LIMITED: cuenta atrás y el reintento se activa al terminar', async () => {
    vi.useFakeTimers({ toFake: ['setInterval', 'clearInterval'] });
    const { api } = await setup({ scenario: 'pr-rate-limit' });
    const alert = (await screen.findByText('Demasiadas peticiones')).closest('.prerr') as HTMLElement;
    expect(alert).toHaveTextContent('Demasiadas peticiones');
    expect(alert).toHaveTextContent('Podrás reintentar en 30 s');
    const retry = within(alert).getByRole('button', { name: 'Reintentar' });
    expect(retry).toBeDisabled();
    await vi.advanceTimersByTimeAsync(10_000);
    expect(alert).toHaveTextContent('Podrás reintentar en 20 s');
    await vi.advanceTimersByTimeAsync(20_000);
    expect(alert).toHaveTextContent('Ya puedes reintentar');
    expect(retry).toBeEnabled();
    const spy = vi.spyOn(api, 'listPullRequests');
    retry.click();
    expect(spy).toHaveBeenCalledTimes(1);
  });

  it('NETWORK: avisa y deja reintentar', async () => {
    const { api } = await setup({ scenario: 'pr-network' });
    const alert = await screen.findByRole('alert');
    expect(alert).toHaveTextContent('Sin conexión con el proveedor');
    const spy = vi.spyOn(api, 'listPullRequests');
    await userEvent.click(within(alert).getByRole('button', { name: 'Reintentar' }));
    expect(spy).toHaveBeenCalledTimes(1);
    expect(await screen.findByRole('alert')).toHaveTextContent('Sin conexión');
  });
});
