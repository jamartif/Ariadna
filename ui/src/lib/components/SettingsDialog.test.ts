import { render, screen, waitFor, within } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { toasts } from '$lib/stores/toasts.svelte';
import { settingsUi } from '$lib/stores/settingsPanel.svelte';
import { loadedTab, mockApi, resetGlobals } from '../../test/helpers';
import SettingsDialog from './SettingsDialog.svelte';
import Sidebar from './Sidebar.svelte';
import { branchesOf } from '$lib/stores/branches.svelte';

beforeEach(() => {
  resetGlobals();
  settingsUi.section = 'general';
});
afterEach(resetGlobals);

async function setup(opts = {}, tabId: string | null = 'tab-ariadna') {
  const api = mockApi(opts);
  const onclose = vi.fn();
  render(SettingsDialog, { props: { api, tabId: tabId ?? undefined, onclose } });
  await screen.findByLabelText('Idioma');
  return { api, onclose };
}

const tab = (name: string) => screen.getByRole('tab', { name });

describe('Ajustes: estructura y accesibilidad', () => {
  it('es un diálogo modal con pestañas verticales y un panel asociado', async () => {
    await setup();
    const dlg = screen.getByRole('dialog', { name: 'Ajustes' });
    expect(dlg).toHaveAttribute('aria-modal', 'true');
    const tabs = within(screen.getByRole('tablist')).getAllByRole('tab');
    expect(tabs.map((t) => t.textContent?.trim())).toEqual(['General', 'Git', 'Inteligencia artificial', 'Cuentas y credenciales', 'Acerca de']);
    expect(tab('General')).toHaveAttribute('aria-selected', 'true');
    expect(screen.getByRole('tabpanel')).toHaveAttribute('aria-labelledby', 'settings-tab-general');
  });

  it('las flechas, Inicio y Fin cambian de sección y mueven el foco', async () => {
    await setup();
    tab('General').focus();
    await userEvent.keyboard('{ArrowDown}');
    expect(tab('Git')).toHaveAttribute('aria-selected', 'true');
    expect(tab('Git')).toHaveFocus();
    await userEvent.keyboard('{End}');
    expect(tab('Acerca de')).toHaveAttribute('aria-selected', 'true');
    await userEvent.keyboard('{ArrowDown}');
    expect(tab('General')).toHaveAttribute('aria-selected', 'true');
    await userEvent.keyboard('{Home}');
    expect(tab('General')).toHaveFocus();
  });

  it('Esc cierra y devuelve el tema que había', async () => {
    const { onclose } = await setup();
    await userEvent.click(screen.getByLabelText('Oscuro'));
    expect(document.documentElement.getAttribute('data-theme')).toBe('dark');
    await userEvent.keyboard('{Escape}');
    expect(onclose).toHaveBeenCalled();
    expect(document.documentElement.hasAttribute('data-theme')).toBe(false);
  });
});

describe('Ajustes: General', () => {
  it('cambiar el tema muestra «Guardar» y lo guarda en Rust', async () => {
    const { api } = await setup();
    expect(screen.queryByRole('button', { name: 'Guardar' })).toBeNull();
    await userEvent.click(screen.getByLabelText('Oscuro'));
    expect(screen.getByText('Hay cambios sin guardar.')).toBeInTheDocument();
    await userEvent.click(screen.getByRole('button', { name: 'Guardar' }));
    await waitFor(async () => expect((await api.getSettings()).settings.theme).toBe('dark'));
    await waitFor(() => expect(screen.queryByRole('button', { name: 'Guardar' })).toBeNull());
  });

  it('tamaño del diff: valida el rango y bloquea Guardar', async () => {
    await setup();
    const size = screen.getByLabelText('Tamaño del texto del diff');
    await userEvent.clear(size);
    await userEvent.type(size, '40');
    expect(size).toHaveAttribute('aria-invalid', 'true');
    expect(screen.getByRole('alert')).toHaveTextContent('entero entre 10 y 24');
    expect(screen.getByRole('button', { name: 'Guardar' })).toBeDisabled();
    await userEvent.clear(size);
    await userEvent.type(size, '16');
    expect(size).toHaveAttribute('aria-invalid', 'false');
    expect(screen.getByRole('button', { name: 'Guardar' })).toBeEnabled();
    expect(document.documentElement.style.getPropertyValue('--diff-font-size')).toBe('16px');
  });
});

describe('Ajustes: Git', () => {
  it('Detectar rellena la ruta y muestra la versión', async () => {
    await setup();
    await userEvent.click(tab('Git'));
    await userEvent.click(screen.getByRole('button', { name: 'Detectar' }));
    expect(await screen.findByText(/Detectado: git 2\.47\.0 en \/usr\/bin\/git/)).toBeInTheDocument();
    expect(screen.getByLabelText('Ejecutable de git')).toHaveValue('/usr/bin/git');
  });

  it('Comprobar una ruta que no existe explica el problema', async () => {
    await setup();
    await userEvent.click(tab('Git'));
    await userEvent.type(screen.getByLabelText('Ejecutable de git'), '/nada/de/nada');
    await userEvent.click(screen.getByRole('button', { name: 'Comprobar' }));
    expect(await screen.findByText(/No hay un ejecutable de git en «\/nada\/de\/nada»/)).toBeInTheDocument();
  });

  it('pull con rebase, identidad en solo lectura', async () => {
    const { api } = await setup();
    await userEvent.click(tab('Git'));
    const rebase = screen.getByRole('checkbox', { name: /Hacer pull con rebase/ });
    expect(rebase).not.toBeChecked();
    await userEvent.click(rebase);
    await userEvent.click(screen.getByRole('button', { name: 'Guardar' }));
    await waitFor(async () => expect((await api.getSettings()).settings.pullRebase).toBe(true));
    const name = await screen.findByDisplayValue('Javier Martínez');
    expect(name).toHaveAttribute('readonly');
    expect(screen.getByDisplayValue('javier@example.com')).toHaveAttribute('readonly');
  });

  it('guardar una ruta inválida muestra el error y no cierra ni limpia', async () => {
    await setup();
    await userEvent.click(tab('Git'));
    await userEvent.type(screen.getByLabelText('Ejecutable de git'), '/ruta/que/no-existe');
    await userEvent.click(screen.getByRole('button', { name: 'Guardar' }));
    expect(await screen.findByRole('alert')).toHaveTextContent(/No se pudieron guardar los ajustes: La ruta de git no es válida/);
  });
});

describe('Ajustes: Inteligencia artificial', () => {
  it('muestra el estado de cada CLI: instalado con versión y ruta, o no encontrado', async () => {
    await setup();
    await userEvent.click(tab('Inteligencia artificial'));
    const claude = (await screen.findByText('Claude')).closest('label') as HTMLElement;
    await waitFor(() => expect(within(claude).getByText('Instalado')).toBeInTheDocument());
    expect(within(claude).getByText(/Versión 2\.1\.319/)).toBeInTheDocument();
    expect(within(claude).getByText(/Ruta: \/usr\/local\/bin\/claude/)).toBeInTheDocument();
    const codex = screen.getByText('Codex').closest('label') as HTMLElement;
    expect(within(codex).getByText('No encontrado')).toBeInTheDocument();
  });

  it('«Iniciar sesión» muestra el comando oficial y se puede copiar', async () => {
    await setup();
    await userEvent.click(tab('Inteligencia artificial'));
    await screen.findByText('Codex');
    const buttons = screen.getAllByRole('button', { name: 'Iniciar sesión' });
    expect(buttons).toHaveLength(3);
    await userEvent.click(buttons[1] as HTMLElement); // Codex
    const group = screen.getByRole('group', { name: 'Inicia sesión en Codex' });
    expect(within(group).getByText('codex login')).toBeInTheDocument();
    expect(group).toHaveTextContent('Ariadna no ve ni guarda tus credenciales');
    const write = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, 'clipboard', { value: { writeText: write }, configurable: true });
    await userEvent.click(within(group).getByRole('button', { name: /Copiar/ }));
    expect(write).toHaveBeenCalledWith('codex login');
  });

  it('activar/desactivar, perfil y proveedor se aplican al momento; Empresa deja Claude desactivado', async () => {
    const { api } = await setup({}, null);
    await userEvent.click(tab('Inteligencia artificial'));
    const enable = await screen.findByRole('checkbox', { name: /Activar la inteligencia artificial/ });
    expect(enable).toBeChecked();
    await userEvent.click(enable);
    await waitFor(() => expect(screen.getByRole('checkbox', { name: /Activar la inteligencia artificial/ })).not.toBeChecked());
    await userEvent.click(screen.getByRole('checkbox', { name: /Activar la inteligencia artificial/ }));
    await userEvent.click(screen.getByLabelText('Empresa', { selector: 'input[type=radio]' }));
    await waitFor(async () => expect((await api.aiUpdateSettings({})).profile.kind).toBe('empresa'));
    const claude = screen.getByText('Claude').closest('.provwrap') as HTMLElement;
    await waitFor(() => expect(within(claude).getByRole('checkbox', { name: 'Habilitado en este perfil' })).not.toBeChecked());
    expect(within(claude).getByText(/Desactivado en el perfil Empresa/)).toBeInTheDocument();
  });

  it('reglas por remoto: lista las de fábrica, añade, rechaza mal formadas y quita', async () => {
    const { api } = await setup();
    await userEvent.click(tab('Inteligencia artificial'));
    const list = await screen.findByRole('list', { name: 'Reglas por remoto' });
    expect(within(list).getByText('dev.azure.com')).toBeInTheDocument();
    expect(within(list).getByText('github.com')).toBeInTheDocument();
    expect(screen.getByText(/En el repositorio actual manda la regla «github.com»: perfil Personal/)).toBeInTheDocument();

    const input = screen.getByLabelText('Nuevo patrón');
    await userEvent.type(input, 'https://gitlab.com');
    await userEvent.click(screen.getByRole('button', { name: 'Añadir regla' }));
    expect(await screen.findByRole('alert')).toHaveTextContent('sin https:// ni usuario');
    await userEvent.clear(input);
    await userEvent.type(input, 'GitLab.com/Acme');
    await userEvent.selectOptions(screen.getByLabelText('Perfil', { selector: '#set-rule-profile' }), 'empresa');
    await userEvent.click(screen.getByRole('button', { name: 'Añadir regla' }));
    expect(within(screen.getByRole('list', { name: 'Reglas por remoto' })).getByText('gitlab.com/acme')).toBeInTheDocument();

    await userEvent.click(screen.getByRole('button', { name: 'Quitar la regla github.com' }));
    expect(screen.queryByText('github.com', { selector: '.pat' })).toBeNull();
    await userEvent.click(screen.getByRole('button', { name: 'Guardar' }));
    await waitFor(async () => expect((await api.getSettings()).settings.profileRules.map((r) => r.pattern)).toEqual(['dev.azure.com', 'gitlab.com/acme']));
  });
});

describe('Ajustes: Cuentas, Acerca de y aviso de archivo dañado', () => {
  it('Cuentas: dice que Ariadna no guarda tokens y lista los 4 proveedores con enlace y permisos', async () => {
    const { api } = await setup();
    const open = vi.spyOn(api, 'openExternalUrl').mockResolvedValue(undefined);
    await userEvent.click(tab('Cuentas y credenciales'));
    expect(screen.getByText(/Ariadna no guarda tokens ni contraseñas/)).toBeInTheDocument();
    for (const n of ['Azure DevOps', 'GitHub', 'GitLab', 'Bitbucket']) expect(screen.getByText(n)).toBeInTheDocument();
    expect(screen.getAllByText('Para subir cambios')).toHaveLength(4);
    expect(screen.getByText('Code → Read.')).toBeInTheDocument();
    await userEvent.click(screen.getByRole('button', { name: 'Abrir guía de GitHub' }));
    expect(open).toHaveBeenCalledWith(expect.stringMatching(/^https:\/\/docs\.github\.com\//));
  });

  it('Acerca de: versión, licencia MIT, versión de git y enlaces https', async () => {
    const { api } = await setup();
    const open = vi.spyOn(api, 'openExternalUrl').mockResolvedValue(undefined);
    await userEvent.click(tab('Acerca de'));
    expect(screen.getByText('0.1.0-alpha.1')).toBeInTheDocument();
    expect(screen.getByText('MIT')).toBeInTheDocument();
    expect(screen.getByText('2.47.0 (/usr/bin/git)')).toBeInTheDocument();
    await userEvent.click(screen.getByRole('button', { name: /Código fuente/ }));
    expect(open).toHaveBeenCalledWith('https://github.com/jamartif/Ariadna');
  });

  it('un archivo de ajustes dañado se avisa con la copia .bak', async () => {
    await setup({ scenario: 'settings-corrupt' });
    expect(screen.getByText('El archivo de ajustes estaba dañado')).toBeInTheDocument();
    expect(screen.getByText(/settings\.json\.bak/)).toBeInTheDocument();
  });
});

describe('Barra lateral', () => {
  it('ya no anuncia que cambiar de rama, crear y fusionar «llegan en la siguiente versión»', async () => {
    const { tab } = await loadedTab();
    const snap = tab.snapshot!;
    render(Sidebar, { props: { tab, snapshot: snap, loading: false } });
    expect(screen.queryByText(/siguiente versión/)).toBeNull();
  });

  it('al pulsar una rama abre el panel de Ramas con esa rama seleccionada', async () => {
    const { tab } = await loadedTab();
    const snap = tab.snapshot!;
    render(Sidebar, { props: { tab, snapshot: snap, loading: false } });
    const name = snap.branches[snap.branches.length - 1]!.name;
    await userEvent.click(screen.getByRole('button', { name: new RegExp(name) }));
    expect(tab.panel).toBe('branches');
    expect(branchesOf(tab).selected).toBe(name);
  });
});

describe('Ajustes: IA con perfiles aislados, cuenta e inicio de sesión (alpha.2)', () => {
  const claudeBox = () => screen.getByText('Claude').closest('.provwrap') as HTMLElement;
  const open = async (opts = {}) => {
    const s = await setup(opts, null);
    await userEvent.click(tab('Inteligencia artificial'));
    await screen.findByText('Codex');
    return s;
  };

  it('Personal usa la sesión normal y muestra la cuenta que informa el CLI', async () => {
    await open();
    const claude = claudeBox();
    expect(within(claude).getByText('Sesión normal del CLI')).toBeInTheDocument();
    await waitFor(() => expect(within(claude).getByText('Sesión iniciada como javier@example.com')).toBeInTheDocument());
    // Lo que el CLI no expone es «no disponible», no una cuenta inventada.
    const codex = screen.getByText('Codex').closest('.provwrap') as HTMLElement;
    expect(within(codex).getByText('Cuenta no disponible')).toBeInTheDocument();
    expect(screen.getByRole('checkbox', { name: /Sesión propia para este perfil/ })).not.toBeChecked();
  });

  it('Empresa aísla la sesión de cada CLI que lo permite y avisa de Copilot, que comparte login', async () => {
    const { api } = await open();
    await userEvent.click(screen.getByLabelText('Empresa', { selector: 'input[type=radio]' }));
    await waitFor(() => expect(screen.getByRole('checkbox', { name: /Sesión propia para este perfil/ })).toBeChecked());
    const codex = screen.getByText('Codex').closest('.provwrap') as HTMLElement;
    await waitFor(() => expect(within(codex).getByText('Sesión propia')).toBeInTheDocument());
    // Codex: aislamiento según la documentación, aún no verificado con el CLI real.
    expect(within(codex).getByText(/no verificado/)).toBeInTheDocument();
    const copilot = screen.getByText('Copilot').closest('.provwrap') as HTMLElement;
    expect(within(copilot).getByText('Comparte sesión')).toBeInTheDocument();
    expect(within(copilot).getByRole('note')).toHaveTextContent('no permite aislar su inicio de sesión: Personal y Empresa usan la misma cuenta');
    expect((await api.aiUpdateSettings({})).profile.isolated).toBe(true);
  });

  it('se puede apagar y encender el aislamiento del perfil', async () => {
    const { api } = await open();
    await userEvent.click(screen.getByRole('checkbox', { name: /Sesión propia para este perfil/ }));
    await waitFor(async () => expect((await api.aiUpdateSettings({})).profile.isolated).toBe(true));
    await waitFor(() => expect(within(claudeBox()).getByText('Sesión propia')).toBeInTheDocument());
    await userEvent.click(screen.getByRole('checkbox', { name: /Sesión propia para este perfil/ }));
    await waitFor(async () => expect((await api.aiUpdateSettings({})).profile.isolated).toBe(false));
  });

  it('«Iniciar sesión» muestra el comando del entorno del perfil y «Abrir terminal» lanza el login oficial', async () => {
    const { api } = await open();
    await userEvent.click(within(claudeBox()).getByRole('button', { name: 'Iniciar sesión' }));
    const group = await screen.findByRole('group', { name: 'Inicia sesión en Claude' });
    await waitFor(() => expect(within(group).getByText('claude auth login')).toBeInTheDocument());
    expect(group).toHaveTextContent('Ariadna no ve ni guarda tus credenciales');
    expect(api.__aiLogins().every((l) => !l.launched)).toBe(true);

    await userEvent.click(within(group).getByRole('button', { name: 'Abrir terminal' }));
    await waitFor(() => expect(api.__aiLogins().at(-1)).toMatchObject({ provider: 'claude', launched: true }));
    await waitFor(() => expect(toasts.items.at(-1)?.title).toMatch(/Se ha abierto una terminal con el inicio de sesión oficial/));
  });

  it('si el CLI no está instalado el login explica por qué y no abre terminal', async () => {
    const { api } = await open();
    const codex = screen.getByText('Codex').closest('.provwrap') as HTMLElement;
    await userEvent.click(within(codex).getByRole('button', { name: 'Iniciar sesión' }));
    const group = await screen.findByRole('group', { name: 'Inicia sesión en Codex' });
    expect(await within(group).findByRole('alert')).toHaveTextContent('No se encontró el comando «codex»');
    expect(within(group).getByText('codex login')).toBeInTheDocument();
    expect(within(group).getByRole('button', { name: 'Abrir terminal' })).toBeDisabled();
    expect(api.__aiLogins()).toHaveLength(0);
  });

  it('el comando de login de Copilot avisa de que comparte sesión y de que no está verificado', async () => {
    await open();
    const copilot = screen.getByText('Copilot').closest('.provwrap') as HTMLElement;
    await userEvent.click(within(copilot).getByRole('button', { name: 'Iniciar sesión' }));
    const group = await screen.findByRole('group', { name: 'Inicia sesión en Copilot' });
    await waitFor(() => expect(group).toHaveTextContent('comparten el mismo login'));
    expect(group).toHaveTextContent('no está verificado con el CLI real');
  });

  it('si no se puede abrir la terminal queda el comando para copiarlo', async () => {
    await open({ scenario: 'ai-no-terminal' });
    await userEvent.click(within(claudeBox()).getByRole('button', { name: 'Iniciar sesión' }));
    const group = await screen.findByRole('group', { name: 'Inicia sesión en Claude' });
    await userEvent.click(await within(group).findByRole('button', { name: 'Abrir terminal' }));
    await waitFor(() => expect(group).toHaveTextContent('No se pudo abrir la terminal'));
    expect(within(group).getByText('claude auth login')).toBeInTheDocument();
  });
});
