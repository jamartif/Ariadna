import { render, screen, waitFor, within } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { branchesOf } from '$lib/stores/branches.svelte';
import { remotesOf } from '$lib/stores/remotes.svelte';
import { loadedTab, resetGlobals } from '../../test/helpers';
import { expectNoSeriousViolations } from '../../test/axe';
import BranchRail from './BranchRail.svelte';
import Toasts from './Toasts.svelte';

beforeEach(resetGlobals);
afterEach(() => {
  resetGlobals();
  vi.restoreAllMocks();
});

/** Riel real (con su menú) sobre el repo simulado `tab-ariadna`. */
async function setup() {
  const ctx = await loadedTab('tab-ariadna');
  const oninspect = vi.fn();
  render(BranchRail, { props: { tab: ctx.tab, collapsed: false, ontoggle: vi.fn(), oninspect, onrepo: vi.fn() } });
  render(Toasts);
  await screen.findByRole('button', { name: /Nueva rama/ });
  const r = remotesOf(ctx.tab);
  await r.load();
  return { ...ctx, oninspect, b: branchesOf(ctx.tab), r };
}

/** Fila de rama por nombre exacto (`main` y `origin/main` conviven). */
function row(name: string): HTMLElement {
  const el = [...document.querySelectorAll<HTMLElement>('[data-brail-name]')].find((b) => b.dataset.brailName === name);
  if (!el) throw new Error(`no hay fila para ${name}`);
  return el;
}

const menu = (): HTMLElement => screen.getByTestId('branch-menu');
const item = (id: string): HTMLElement => screen.getByTestId(`branch-menu-${id}`);
const headName = (tab: Awaited<ReturnType<typeof setup>>['tab']) => tab.snapshot!.branches.find((x) => x.isHead)!.name;

async function rightClick(name: string, coords = { clientX: 120, clientY: 200 }): Promise<HTMLElement> {
  await userEvent.pointer({ keys: '[MouseRight]', target: row(name), coords });
  return screen.findByRole('menu');
}

describe('Menú contextual del riel: abrir y navegar', () => {
  it('se abre con clic derecho, con todas las acciones y el foco en la primera', async () => {
    const { } = await setup();
    const m = await rightClick('main');

    expect(m).toHaveAccessibleName('Acciones de la rama main');
    expect(m).toHaveAttribute('role', 'menu');
    const labels = within(m)
      .getAllByRole('menuitem')
      .map((x) => x.textContent?.trim());
    // Los cinco grupos, en orden: cambiar/crear/fusionar/rebasar · red · escribir · copiar/comparar · web.
    expect(labels).toEqual([
      'Cambiar a esta rama',
      'Crear una rama desde esta…',
      'Fusionar esta rama en la actual',
      'Rebasar la actual sobre esta',
      'Traer del remoto «origin»',
      'Subir la rama',
      'Bajar la rama actual',
      'Renombrar…',
      'Borrar la rama',
      'Borrar en el remoto «origin»',
      'Copiar el nombre',
      'Copiar el SHA',
      'Ver el diff contra «rust-migration»',
      'Comparar con…',
      'Abrir en el navegador',
      'Crear pull request'
    ]);
    expect(within(m).getAllByRole('separator')).toHaveLength(4);
    expect(item('checkout')).toHaveFocus();
  });

  it('se abre con Mayús+F10 y con la tecla Menú, junto a la fila enfocada', async () => {
    await setup();
    row('main').focus();
    await userEvent.keyboard('{Shift>}{F10}{/Shift}');
    expect(await screen.findByRole('menu')).toHaveAccessibleName('Acciones de la rama main');
    await userEvent.keyboard('{Escape}');
    await waitFor(() => expect(screen.queryByRole('menu')).toBeNull());

    row('main').focus();
    await userEvent.keyboard('{ContextMenu}');
    expect(await screen.findByRole('menu')).toHaveAccessibleName('Acciones de la rama main');
  });

  it('↑/↓, Inicio y Fin mueven el foco sin salir del menú', async () => {
    await setup();
    await rightClick('main');
    expect(item('checkout')).toHaveFocus();
    await userEvent.keyboard('{ArrowDown}');
    expect(item('from')).toHaveFocus();
    await userEvent.keyboard('{ArrowUp}');
    expect(item('checkout')).toHaveFocus();
    await userEvent.keyboard('{End}');
    expect(item('pr')).toHaveFocus();
    // El último + ↓ vuelve al primero (no se escapa del menú).
    await userEvent.keyboard('{ArrowDown}');
    expect(item('checkout')).toHaveFocus();
    await userEvent.keyboard('{Home}');
    expect(item('checkout')).toHaveFocus();
  });

  it('Escape cierra y devuelve el foco a la fila', async () => {
    await setup();
    await rightClick('main');
    await userEvent.keyboard('{Escape}');
    await waitFor(() => expect(screen.queryByRole('menu')).toBeNull());
    expect(row('main')).toHaveFocus();
  });

  it('un clic fuera cierra el menú', async () => {
    await setup();
    await rightClick('main');
    await userEvent.click(document.body);
    await waitFor(() => expect(screen.queryByRole('menu')).toBeNull());
  });

  it('se reposiciona para no salirse de la ventana', async () => {
    await setup();
    vi.spyOn(Element.prototype, 'getBoundingClientRect').mockReturnValue({
      x: 0,
      y: 0,
      top: 0,
      left: 0,
      right: 240,
      bottom: 400,
      width: 240,
      height: 400,
      toJSON: () => ({})
    } as DOMRect);
    Object.defineProperty(window, 'innerWidth', { configurable: true, value: 420 });
    Object.defineProperty(window, 'innerHeight', { configurable: true, value: 300 });

    const m = await rightClick('main', { clientX: 400, clientY: 280 });
    // A la derecha no caben 240 px: se abre hacia la izquierda del cursor (400 - 240 = 160).
    expect(m.style.left).toBe('160px');
    // Hacia abajo tampoco cabe: se pega al borde superior con 8 px de margen.
    expect(m.style.top).toBe('8px');
    const left = Number.parseInt(m.style.left, 10);
    const top = Number.parseInt(m.style.top, 10);
    expect(left + 240).toBeLessThanOrEqual(420);
  });
});

describe('Menú contextual del riel: estado de las acciones', () => {
  it('la rama actual deshabilita lo que no aplica, con el motivo en el tooltip', async () => {
    await setup();
    await rightClick('rust-migration');

    for (const id of ['checkout', 'merge', 'rebase', 'diff']) {
      expect(item(id)).toHaveAttribute('aria-disabled', 'true');
      expect(item(id).getAttribute('title')).toBeTruthy();
    }
    // Borrar la rama actual se explica igual que en el inspector.
    expect(item('delete')).toHaveAttribute('aria-disabled', 'true');
    expect(item('delete')).toHaveAttribute('title', 'La rama actual no se puede borrar: cambia antes a otra.');
    // Lo que sí aplica en la actual sigue disponible.
    expect(item('pull')).not.toHaveAttribute('aria-disabled');
    expect(item('push')).not.toHaveAttribute('aria-disabled');
    expect(item('copy-sha')).not.toHaveAttribute('aria-disabled');
  });

  it('una acción deshabilitada no hace nada al pulsarla', async () => {
    const { tab } = await setup();
    await rightClick('rust-migration');
    await userEvent.click(item('checkout'));
    // Sigue el mismo menú abierto y la rama no cambió.
    expect(menu()).toBeInTheDocument();
    expect(headName(tab)).toBe('rust-migration');
  });

  it('las filas del remoto ofrecen borrar en el remoto y no subir en local', async () => {
    await setup();
    await rightClick('origin/main');
    expect(item('checkout')).toHaveAccessibleName('Cambiar a esta rama (crea la local)'.replace('Cambiar', 'Cambiar'));
    expect(item('push')).toHaveAttribute('aria-disabled', 'true');
    expect(item('push')).toHaveAttribute('title', 'Solo para ramas locales.');
    expect(item('delete')).toHaveAttribute('aria-disabled', 'true');
    expect(item('delete-remote')).toHaveAccessibleName('Borrar en el remoto «origin»');
    expect(item('delete-remote')).not.toHaveAttribute('aria-disabled');
    expect(item('pull')).toHaveAttribute('aria-disabled', 'true');
  });
});

describe('Menú contextual del riel: acciones', () => {
  it('cambiar de rama desde el menú (y avisar antes si hay cambios)', async () => {
    const { tab } = await setup();
    await rightClick('main');
    await userEvent.click(item('checkout'));
    // Hay cambios sin confirmar: el riel pide confirmación antes de moverse.
    expect(await screen.findByTestId('rail-checkout-prompt')).toBeInTheDocument();
    expect(headName(tab)).toBe('rust-migration');
    expect(screen.queryByRole('menu')).toBeNull();
    await userEvent.click(screen.getByTestId('rail-switch-anyway'));
    await waitFor(() => expect(headName(tab)).toBe('main'));
  });

  it('crear una rama desde esta prellena el punto de partida', async () => {
    await setup();
    await rightClick('main');
    await userEvent.click(item('from'));
    const form = await screen.findByTestId('rail-create');
    expect(within(form).getByLabelText('Crear desde')).toHaveValue('main');
  });

  it('renombrar abre el formulario del inspector con el nombre de esa rama', async () => {
    await setup();
    await rightClick('feature/command-palette');
    await userEvent.click(item('rename'));
    const input = await screen.findByLabelText('Nuevo nombre');
    expect(input).toHaveValue('feature/command-palette');
  });

  it('borrar la rama usa la confirmación nativa de Rust', async () => {
    const { tab } = await setup();
    await rightClick('feature/command-palette');
    await userEvent.click(item('delete'));
    await waitFor(() => expect(tab.snapshot?.branches.some((x) => x.name === 'feature/command-palette')).toBe(false));
  });

  it('ver el diff contra la actual lo carga y abre el detalle de la rama', async () => {
    const { b, oninspect } = await setup();
    await rightClick('main');
    await userEvent.click(item('diff'));
    await waitFor(() => expect(b.diff).not.toBeNull());
    expect(b.diff).toMatchObject({ base: 'rust-migration', head: 'main' });
    expect(oninspect).toHaveBeenCalledWith('main');
  });

  it('copiar el nombre y el SHA usa el portapapeles', async () => {
    const writes: string[] = [];
    Object.defineProperty(navigator, 'clipboard', {
      configurable: true,
      value: { writeText: (x: string) => (writes.push(x), Promise.resolve()) }
    });
    const { tab } = await setup();
    await rightClick('main');
    await userEvent.click(item('copy-name'));
    await rightClick('main');
    await userEvent.click(item('copy-sha'));
    const main = tab.snapshot!.branches.find((x) => x.name === 'main')!;
    expect(writes).toEqual(['main', main.oid]);
  });

  it('abrir en el navegador y crear la pull request reutilizan open_remote_url', async () => {
    const { api } = await setup();
    await rightClick('main');
    await userEvent.click(item('open'));
    await waitFor(() => expect(api.__openedUrls()).toContain('https://github.com/javie/ariadna/tree/main'));
    await rightClick('main');
    await userEvent.click(item('pr'));
    await waitFor(() => expect(api.__openedUrls()).toContain('https://github.com/javie/ariadna/pull/new/main'));
  });

  it('el segundo paso «Comparar con…» compara contra la rama elegida', async () => {
    const { b, oninspect } = await setup();
    await rightClick('main');
    await userEvent.click(item('compare'));
    const m = menu();
    expect(m).toHaveAttribute('data-mode', 'compare');
    expect(m).toHaveAccessibleName('Comparar «main» con…');
    // La propia rama no aparece como candidata; sí el resto de locales y remotas.
    const names = within(m)
      .getAllByRole('menuitem')
      .map((x) => x.textContent?.trim());
    expect(names).toEqual(['Volver', 'rust-migration', 'feature/command-palette', 'origin/rust-migration', 'origin/main']);

    await userEvent.click(item('cmp:feature/command-palette'));
    await waitFor(() => expect(b.diff).not.toBeNull());
    expect(b.diff).toMatchObject({ base: 'feature/command-palette', head: 'main' });
    expect(oninspect).toHaveBeenCalledWith('main');
  });

  it('«Volver» del segundo paso devuelve al menú de acciones', async () => {
    await setup();
    await rightClick('main');
    await userEvent.click(item('compare'));
    await userEvent.click(item('back'));
    expect(menu()).toHaveAttribute('data-mode', 'actions');
    expect(item('checkout')).toBeInTheDocument();
  });

  it('abrir el menú de otra fila sin cerrarlo antes lo recoloca y vuelve al primer paso', async () => {
    await setup();
    await rightClick('main', { clientX: 100, clientY: 200 });
    await userEvent.click(item('compare'));
    expect(menu()).toHaveAttribute('data-mode', 'compare');
    // Con el teclado no hay `pointerdown` fuera: el riel reutiliza el menú con las props nuevas.
    row('feature/command-palette').focus();
    await userEvent.keyboard('{Shift>}{F10}{/Shift}');
    await waitFor(() => expect(menu()).toHaveAccessibleName('Acciones de la rama feature/command-palette'));
    expect(menu()).toHaveAttribute('data-mode', 'actions');
    expect(item('checkout')).toHaveFocus();
  });

  it('el menú abierto no tiene violaciones de accesibilidad', async () => {
    await setup();
    await rightClick('main');
    await expectNoSeriousViolations();
  });
});
