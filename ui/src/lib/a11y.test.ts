import { cleanup, render, screen, waitFor, within } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, describe, it, vi } from 'vitest';
import { branchesOf } from '$lib/stores/branches.svelte';
import { conflictsOf } from '$lib/stores/conflicts.svelte';
import { prOf } from '$lib/stores/pullRequests.svelte';
import { settingsUi } from '$lib/stores/settingsPanel.svelte';
import { buildActions } from '$lib/stores/commands';
import { AppStore } from '$lib/stores/app.svelte';
import { expect } from 'vitest';
import { expectNoSeriousViolations } from '../test/axe';
import { loadedTab, mockApi, resetGlobals } from '../test/helpers';
import { prTab } from '../test/prHelpers';
import BranchDetail from './components/BranchDetail.svelte';
import BranchRail from './components/BranchRail.svelte';
import CommandPalette from './components/CommandPalette.svelte';
import DiffView from './components/DiffView.svelte';
import ConflictEditor from './components/ConflictEditor.svelte';
import ConflictsPanel from './components/ConflictsPanel.svelte';
import HistoryDetail from './components/HistoryDetail.svelte';
import HistoryPanel from './components/HistoryPanel.svelte';
import PrDetail from './components/PrDetail.svelte';
import PullRequestsPanel from './components/PullRequestsPanel.svelte';
import RemotesSection from './components/RemotesSection.svelte';
import RepoDialog from './components/RepoDialog.svelte';
import TagsSection from './components/TagsSection.svelte';
import SettingsDialog from './components/SettingsDialog.svelte';
import Workspace from './components/Workspace.svelte';

/**
 * Accesibilidad automatizada (axe-core) de las pantallas principales: sin violaciones `serious`/`critical`.
 * No sustituye la revisión manual con NVDA/VoiceOver (pendiente, M7) ni el contraste en navegador.
 */
beforeEach(resetGlobals);
afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
  settingsUi.close();
  resetGlobals();
});

describe('axe: Pull requests', () => {
  async function setup(opts: Parameters<typeof prTab>[0] = {}) {
    const ctx = await prTab(opts);
    render(PullRequestsPanel, { props: { tab: ctx.tab } });
    render(PrDetail, { props: { tab: ctx.tab } });
    return ctx;
  }

  it('lista y estado «elige una PR»', async () => {
    await setup();
    await screen.findByRole('button', { name: /Añade validación de formularios/ });
    await expectNoSeriousViolations();
  });

  it('lista vacía (pr-empty)', async () => {
    await setup({ scenario: 'pr-empty' });
    await screen.findByText('No hay pull requests');
    await expectNoSeriousViolations();
  });

  it.each(['pr-auth', 'pr-rate-limit', 'pr-network'])('error de carga (%s)', async (scenario) => {
    await setup({ scenario });
    await screen.findByText(/Falta autorización|Demasiadas peticiones|Sin conexión con el proveedor/);
    await expectNoSeriousViolations();
  });

  it('detalle: resumen, archivos con hilos y conversación', async () => {
    const { tab } = await setup();
    const user = userEvent.setup();
    await user.click(await screen.findByRole('button', { name: /Añade validación de formularios/ }));
    await screen.findByRole('heading', { level: 2, name: /Añade validación/ });
    await expectNoSeriousViolations();
    const pr = prOf(tab);
    for (const view of ['files', 'conversation'] as const) {
      pr.setView(view);
      await waitFor(() => expect(screen.getByRole('tab', { selected: true })).toBeInTheDocument());
      await expectNoSeriousViolations();
    }
  });

  it('formularios: crear, votar y completar', async () => {
    const { tab } = await setup();
    const user = userEvent.setup();
    const pr = prOf(tab);
    pr.startCreate();
    await screen.findByRole('heading', { name: /Nueva pull request/ });
    await expectNoSeriousViolations();
    pr.cancelCreate?.();
    if (pr.creating) pr.closeDetail();
    await user.click(await screen.findByRole('button', { name: /Añade validación de formularios/ }));
    await user.click(await screen.findByRole('button', { name: /Revisar/ }));
    await screen.findByRole('group', { name: /Tu revisión/ });
    await expectNoSeriousViolations();
    await user.click(screen.getByRole('button', { name: /Completar$/ }));
    await screen.findByRole('group', { name: /Estrategia de fusión/ });
    await expectNoSeriousViolations();
  });
});

describe('axe: Conflictos', () => {
  it('lista, editor de tres paneles y archivo binario', async () => {
    const ctx = await loadedTab('tab-legacy');
    ctx.tab.panel = 'conflicts';
    render(ConflictsPanel, { props: { tab: ctx.tab } });
    render(ConflictEditor, { props: { tab: ctx.tab } });
    await waitFor(() => expect(conflictsOf(ctx.tab).selected).not.toBeNull());
    await screen.findByRole('textbox', { name: /^Resultado editable/ });
    await expectNoSeriousViolations();
    const user = userEvent.setup();
    await user.click(screen.getByRole('button', { name: /logo\.png/ }));
    await screen.findByText(/Archivo binario\. Este conflicto/);
    await expectNoSeriousViolations();
  });
});

describe('axe: Ajustes', () => {
  it.each(['General', 'Git', 'Inteligencia artificial', 'Cuentas y credenciales', 'Acerca de'])('sección %s', async (name) => {
    settingsUi.section = 'general';
    render(SettingsDialog, { props: { api: mockApi(), tabId: 'tab-ariadna', onclose: vi.fn() } });
    await screen.findByLabelText('Idioma');
    const user = userEvent.setup();
    await user.click(screen.getByRole('tab', { name }));
    await waitFor(() => expect(screen.getByRole('tab', { name })).toHaveAttribute('aria-selected', 'true'));
    await expectNoSeriousViolations();
  });
});

describe('axe: Ajustes en inglés', () => {
  it('General con la interfaz en inglés (idioma recién cambiado)', async () => {
    settingsUi.section = 'general';
    render(SettingsDialog, { props: { api: mockApi(), tabId: 'tab-ariadna', onclose: vi.fn() } });
    await screen.findByLabelText('Idioma');
    const user = userEvent.setup();
    await user.selectOptions(screen.getByLabelText('Idioma'), 'en');
    await screen.findByRole('dialog', { name: 'Settings' });
    await user.click(screen.getByRole('tab', { name: 'General' }));
    await waitFor(() => expect(screen.getByRole('tab', { name: 'General' })).toHaveAttribute('aria-selected', 'true'));
    expect(screen.getByLabelText('Diff text size')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Save' })).toBeInTheDocument();
    await expectNoSeriousViolations();
  });
});

describe('axe: riel de ramas, detalle y remotos', () => {
  it('riel con rama actual, seguimiento y inspector', async () => {
    const ctx = await loadedTab('tab-ariadna');
    render(BranchRail, {
      props: { tab: ctx.tab, collapsed: false, ontoggle: vi.fn(), oninspect: vi.fn(), onrepo: vi.fn() }
    });
    render(BranchDetail, { props: { tab: ctx.tab } });
    await screen.findByRole('button', { name: /Nueva rama/ });
    const nav = await screen.findByRole('navigation', { name: 'Árbol de ramas' });
    expect(within(nav).getByText('(rama actual)', { exact: false })).toBeInTheDocument();
    await expectNoSeriousViolations();
    void branchesOf(ctx.tab);
  });

  it('riel contraído (solo iconos)', async () => {
    const ctx = await loadedTab('tab-ariadna');
    render(BranchRail, {
      props: { tab: ctx.tab, collapsed: true, ontoggle: vi.fn(), oninspect: vi.fn(), onrepo: vi.fn() }
    });
    await screen.findByRole('button', { name: 'Expandir el riel de ramas' });
    await expectNoSeriousViolations();
  });

  it('diálogo de remotos y etiquetas', async () => {
    const ctx = await loadedTab('tab-ariadna');
    render(RepoDialog, { props: { tab: ctx.tab, onclose: vi.fn() } });
    await screen.findByRole('dialog', { name: 'Remotos y etiquetas' });
    await screen.findByRole('region', { name: 'Etiquetas del repositorio' });
    await expectNoSeriousViolations();
  });

  it('formulario de añadir remoto', async () => {
    const ctx = await loadedTab('tab-ariadna');
    render(RemotesSection, { props: { tab: ctx.tab } });
    await userEvent.click(await screen.findByRole('button', { name: /Añadir remoto/ }));
    await screen.findByLabelText(/Nombre/);
    await expectNoSeriousViolations();
  });
});

describe('axe: menú contextual del riel', () => {
  it('menú de acciones abierto (rama actual, con acciones deshabilitadas)', async () => {
    const ctx = await loadedTab('tab-ariadna');
    render(BranchRail, {
      props: { tab: ctx.tab, collapsed: false, ontoggle: vi.fn(), oninspect: vi.fn(), onrepo: vi.fn() }
    });
    const nav = await screen.findByRole('navigation', { name: 'Árbol de ramas' });
    const currentRow = [...nav.querySelectorAll<HTMLElement>('[data-brail-name]')].find((r) => r.dataset.brailName === 'rust-migration');
    expect(currentRow).toBeDefined();
    await userEvent.pointer({ keys: '[MouseRight]', target: currentRow as HTMLElement, coords: { clientX: 60, clientY: 120 } });
    const menu = await screen.findByRole('menu');
    expect(within(menu).getAllByRole('menuitem').length).toBeGreaterThan(10);
    await expectNoSeriousViolations();
  });
});

describe('axe: Historial', () => {
  it('lista de commits y detalle', async () => {
    const ctx = await loadedTab('tab-ariadna');
    render(HistoryPanel, { props: { tab: ctx.tab } });
    render(HistoryDetail, { props: { tab: ctx.tab } });
    const list = await screen.findByRole('listbox', { name: 'Lista de commits' });
    await waitFor(() => expect(list.querySelectorAll('[role=option]').length).toBeGreaterThan(2));
    await expectNoSeriousViolations();
  });
});

describe('axe: paleta de comandos y espacio de trabajo', () => {
  it('paleta de comandos', async () => {
    const app = new AppStore(mockApi());
    await app.init();
    render(CommandPalette, { props: { actions: buildActions(app), onclose: vi.fn() } });
    await screen.findByRole('dialog');
    await expectNoSeriousViolations();
  });

  it('espacio de trabajo con las pestañas de panel', async () => {
    const ctx = await loadedTab('tab-ariadna');
    render(Workspace, { props: { tab: ctx.tab, onclose: vi.fn(), onlocate: vi.fn() } });
    await screen.findByRole('tablist', { name: 'Secciones del repositorio' });
    await expectNoSeriousViolations();
  });
});

describe('foco en formularios de PR', () => {
  it('entra en el formulario al abrirlo, Escape lo cierra y el foco vuelve al botón', async () => {
    const ctx = await prTab();
    render(PullRequestsPanel, { props: { tab: ctx.tab } });
    render(PrDetail, { props: { tab: ctx.tab } });
    const user = userEvent.setup();
    await user.click(await screen.findByRole('button', { name: /Añade validación de formularios/ }));
    const open = await screen.findByRole('button', { name: /Revisar/ });
    await user.click(open);
    const form = await screen.findByRole('group', { name: /Tu revisión/ });
    await waitFor(() => expect(form.contains(document.activeElement)).toBe(true));
    await user.keyboard('{Escape}');
    await waitFor(() => expect(screen.queryByRole('group', { name: /Tu revisión/ })).toBeNull());
    await waitFor(() => expect(document.activeElement).toBe(open));
  });
});

describe('axe: líneas sueltas y stash', () => {
  it('diff con casillas por línea y una selección activa', async () => {
    const { tab } = await loadedTab();
    const e = tab.sections.unstaged.find((x) => x.path === 'src/lib/api/types.ts')!;
    tab.select('unstaged', e);
    render(DiffView, { props: { tab } });
    const boxes = await screen.findAllByRole('checkbox', { name: /^Seleccionar la línea/ });
    await userEvent.click(boxes[1] as HTMLElement);
    await screen.findAllByRole('button', { name: 'Preparar líneas seleccionadas (1)' });
    await expectNoSeriousViolations();
  });

  it('espacio de trabajo con el panel de Stashes abierto', async () => {
    const { tab } = await loadedTab();
    await tab.api.stashPush(tab.id, { message: 'axe', includeUntracked: true });
    tab.panel = 'stashes';
    render(Workspace, { props: { tab, onclose: () => {}, onlocate: () => {} } });
    await screen.findByRole('heading', { level: 2, name: 'axe' });
    await expectNoSeriousViolations(document.body, ['aria-allowed-role']);
  });
});

describe('axe: etiquetas', () => {
  it('lista de etiquetas con el formulario de nueva etiqueta abierto', async () => {
    const ctx = await loadedTab('tab-ariadna');
    render(TagsSection, { props: { tab: ctx.tab } });
    await screen.findByRole('button', { name: 'Publicar la etiqueta «v0.2.0»' });
    await userEvent.click(screen.getByRole('button', { name: 'Nueva etiqueta' }));
    await userEvent.type(await screen.findByLabelText('Nombre'), 'mala etiqueta');
    await screen.findByText('El nombre no puede tener espacios.');
    await expectNoSeriousViolations();
  });

  it('detalle del commit con «Crear etiqueta aquí» abierto', async () => {
    const ctx = await loadedTab('tab-ariadna');
    render(HistoryPanel, { props: { tab: ctx.tab } });
    render(HistoryDetail, { props: { tab: ctx.tab } });
    const list = await screen.findByRole('listbox', { name: 'Lista de commits' });
    await waitFor(() => expect(list.querySelectorAll('[role="option"]').length).toBeGreaterThan(2));
    await userEvent.click(await screen.findByRole('button', { name: 'Crear etiqueta aquí' }));
    await screen.findByLabelText('Mensaje (opcional)');
    await expectNoSeriousViolations();
  });
});

