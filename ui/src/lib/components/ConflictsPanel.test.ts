import { render, screen, waitFor, within } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { conflictsOf } from '$lib/stores/conflicts.svelte';
import { dispatchKey, installShortcuts } from '$lib/shortcuts/registry';
import { loadedTab, resetGlobals } from '../../test/helpers';
import ConflictEditor from './ConflictEditor.svelte';
import ConflictsPanel from './ConflictsPanel.svelte';
import Toasts from './Toasts.svelte';

beforeEach(resetGlobals);
afterEach(resetGlobals);

async function setup(confirm: () => boolean = () => true) {
  const ctx = await loadedTab('tab-legacy', { confirm });
  ctx.tab.panel = 'conflicts';
  render(ConflictsPanel, { props: { tab: ctx.tab } });
  render(ConflictEditor, { props: { tab: ctx.tab } });
  render(Toasts);
  await waitFor(() => expect(conflictsOf(ctx.tab).selected).not.toBeNull());
  return ctx;
}

const list = () => screen.getByRole('list', { name: 'Archivos en conflicto' });
const fileBtn = (name: string) => within(list()).getByRole('button', { name: new RegExp(name.replace('.', '\\.')) });
const result = () => screen.getByRole('textbox', { name: /^Resultado editable/ }) as HTMLTextAreaElement;

describe('Estudio de conflictos', () => {
  it('lista los archivos con su tipo y el progreso; continuar está bloqueado', async () => {
    await setup();
    const items = within(list()).getAllByRole('listitem');
    expect(items).toHaveLength(4);
    expect(within(list()).getByText('Eliminado por ellos')).toBeInTheDocument();
    expect(within(list()).getByText('Ambos añadieron')).toBeInTheDocument();
    expect(screen.getByText('0 de 4 resueltos')).toBeInTheDocument();
    expect(screen.getByText('Quedan 4 archivos por resolver')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Crear commit de fusión' })).toBeDisabled();
    expect(screen.getByRole('heading', { level: 2, name: 'Fusión de «origin/main» en «feature/payments»' })).toBeInTheDocument();
  });

  it('muestra ours | base | theirs con el resultado editable y bloques pendientes', async () => {
    const { tab } = await setup();
    await userEvent.click(fileBtn('invoice.ts'));
    expect(await screen.findByText('4 bloques pendientes')).toBeInTheDocument();
    for (const side of ['Nuestro', 'Base', 'Suyo']) {
      expect(screen.getByRole('textbox', { name: new RegExp(`^${side} de src/billing/invoice.ts`) })).toBeInTheDocument();
    }
    expect(result().value).toContain('<<<<<<< HEAD');
    // «Marcar como resuelto» deshabilitado mientras queden marcadores.
    const mark = screen.getByRole('button', { name: 'Marcar como resuelto' });
    expect(mark).toBeDisabled();
    expect(screen.getByText(/Quedan marcadores de conflicto/)).toBeInTheDocument();
    expect(conflictsOf(tab).pending(conflictsOf(tab).selected!)).toBe(4);
  });

  it('resuelve bloque a bloque (nuestro, suyo, ambos) con navegación y habilita Marcar como resuelto', async () => {
    const { tab, api } = await setup();
    await userEvent.click(fileBtn('invoice.ts'));
    await screen.findByText('4 bloques pendientes');
    expect(screen.getByText('Bloque 1 de 4')).toBeInTheDocument();

    await userEvent.click(screen.getByRole('button', { name: 'Aceptar nuestro' }));
    expect(await screen.findByText('3 bloques pendientes')).toBeInTheDocument();
    expect(result().value).toContain("import { applyDiscount } from './discount';");

    await userEvent.click(screen.getByRole('button', { name: 'Siguiente' }));
    expect(screen.getByText('Bloque 2 de 3')).toBeInTheDocument();
    await userEvent.click(screen.getByRole('button', { name: 'Aceptar ambos' }));
    expect(await screen.findByText('2 bloques pendientes')).toBeInTheDocument();
    await userEvent.click(screen.getByRole('button', { name: 'Aceptar suyo' }));
    expect(await screen.findByText('1 bloque pendiente')).toBeInTheDocument();
    expect(screen.getByText('Bloque 1 de 1')).toBeInTheDocument();
    await userEvent.click(screen.getByRole('button', { name: 'Aceptar nuestro' }));
    expect(await screen.findByText('Sin bloques pendientes')).toBeInTheDocument();
    expect(result().value).not.toMatch(/<<<<<<<|>>>>>>>/);
    expect(conflictsOf(tab).isDirty(conflictsOf(tab).selected!)).toBe(true);

    const mark = screen.getByRole('button', { name: 'Marcar como resuelto' });
    expect(mark).toBeEnabled();
    await userEvent.click(mark);
    await waitFor(() => expect(within(list()).queryByRole('button', { name: /invoice\.ts/ })).not.toBeInTheDocument());
    expect(await screen.findByText('1 de 4 resueltos')).toBeInTheDocument();
    expect((await api.mergeState(tab.id)).conflicts).toHaveLength(3);
  });

  it('los atajos Alt+↓ / Alt+1 / Alt+2 funcionan con el foco en el resultado', async () => {
    await setup();
    const off = installShortcuts(window, false);
    await userEvent.click(fileBtn('invoice.ts'));
    await screen.findByText('4 bloques pendientes');
    result().focus();
    await userEvent.keyboard('{Alt>}{ArrowDown}{/Alt}');
    expect(screen.getByText('Bloque 2 de 4')).toBeInTheDocument();
    await userEvent.keyboard('{Alt>}1{/Alt}');
    expect(await screen.findByText('3 bloques pendientes')).toBeInTheDocument();
    await userEvent.keyboard('{Alt>}2{/Alt}');
    expect(await screen.findByText('2 bloques pendientes')).toBeInTheDocument();
    // Fuera del editor no hace nada.
    expect(dispatchKey(new KeyboardEvent('keydown', { key: '1', altKey: true }), false)).toBe(false);
    off();
  });

  it('borrado vs. modificado y binarios: se elige una versión entera, sin editor', async () => {
    const { tab } = await setup();
    await userEvent.click(fileBtn('tax.ts'));
    expect(await screen.findByText(/Un lado borró el archivo y el otro lo modificó/)).toBeInTheDocument();
    expect(screen.queryByRole('textbox', { name: /^Resultado editable/ })).not.toBeInTheDocument();
    expect(screen.getByText('No existe (borrado en este lado)')).toBeInTheDocument();
    await userEvent.click(screen.getByRole('button', { name: 'Aceptar el borrado' }));
    await waitFor(() => expect(tab.snapshot!.conflicts.some((f) => f.path === 'src/billing/tax.ts')).toBe(false));
    expect(tab.snapshot!.status.entries.some((e) => e.path === 'src/billing/tax.ts')).toBe(false);

    await userEvent.click(fileBtn('logo.png'));
    expect(await screen.findAllByText('Archivo binario')).not.toHaveLength(0);
    expect(screen.getByRole('button', { name: 'Conservar la suya' })).toBeInTheDocument();
    await userEvent.click(screen.getByRole('button', { name: 'Conservar la nuestra' }));
    await waitFor(() => expect(tab.snapshot!.conflicts.some((f) => f.path === 'assets/logo.png')).toBe(false));
  });

  it('si Rust cancela el diálogo nativo no se resuelve nada', async () => {
    const { tab } = await setup(() => false);
    await userEvent.click(fileBtn('logo.png'));
    await userEvent.click(await screen.findByRole('button', { name: 'Conservar la suya' }));
    await new Promise((r) => setTimeout(r, 30));
    expect(tab.snapshot!.conflicts).toHaveLength(4);
    expect(within(list()).getAllByRole('listitem')).toHaveLength(4);
  });

  it('resolver todo y continuar crea el commit de fusión con el mensaje editable y vuelve a Cambios', async () => {
    const { tab, api } = await setup();
    const c = conflictsOf(tab);
    for (const f of [...tab.snapshot!.conflicts]) await api.resolveConflict(tab.id, f.path, { side: 'ours' });
    await waitFor(() => expect(tab.snapshot!.conflicts).toHaveLength(0));
    const msg = await screen.findByRole('textbox', { name: 'Mensaje del commit de fusión' });
    expect((msg as HTMLTextAreaElement).value).toContain("Merge branch 'origin/main' into feature/payments");
    await userEvent.clear(msg);
    await userEvent.type(msg, 'Integra main (revisado)');
    const go = screen.getByRole('button', { name: 'Crear commit de fusión' });
    expect(go).toBeEnabled();
    await userEvent.click(go);
    await waitFor(() => expect(tab.snapshot!.integration).toBe('none'));
    expect(tab.panel).toBe('changes');
    expect((await api.log(tab.id, 1, null))[0]?.subject).toBe('Integra main (revisado)');
    expect(c.active).toBe(false);
  });

  it('continuar con conflictos pendientes lo rechaza Rust con CONFLICTS_PENDING', async () => {
    const { tab, api } = await setup();
    await expect(api.mergeContinue(tab.id, null)).rejects.toMatchObject({ code: 'CONFLICTS_PENDING' });
  });

  it('abortar con ediciones sin aplicar avisa antes; Rust confirma y el repo vuelve a como estaba', async () => {
    const { tab } = await setup();
    const c = conflictsOf(tab);
    await userEvent.click(fileBtn('invoice.ts'));
    await screen.findByText('4 bloques pendientes');
    await userEvent.click(screen.getByRole('button', { name: 'Aceptar nuestro' }));
    expect(c.dirtyCount).toBe(1);

    await userEvent.click(screen.getByRole('button', { name: 'Abortar la fusión' }));
    const warn = await screen.findByRole('alertdialog');
    expect(within(warn).getByText('1 archivo con ediciones sin aplicar que se perderán')).toBeInTheDocument();
    expect(tab.snapshot!.integration).toBe('merging');

    await userEvent.click(within(warn).getByRole('button', { name: 'Seguir resolviendo' }));
    expect(screen.queryByRole('alertdialog')).not.toBeInTheDocument();

    await userEvent.click(screen.getByRole('button', { name: 'Abortar la fusión' }));
    await userEvent.click(within(await screen.findByRole('alertdialog')).getByRole('button', { name: 'Abortar de todos modos' }));
    await waitFor(() => expect(tab.snapshot!.integration).toBe('none'));
    expect(tab.snapshot!.conflicts).toHaveLength(0);
    expect(tab.sections.conflicts).toHaveLength(0);
    expect(tab.panel).toBe('changes');
  });

  it('si el diálogo nativo de abortar se cancela, sigue el merge', async () => {
    const { tab } = await setup(() => false);
    await userEvent.click(screen.getByRole('button', { name: 'Abortar la fusión' }));
    await new Promise((r) => setTimeout(r, 30));
    expect(tab.snapshot!.integration).toBe('merging');
  });

  it('descartar las ediciones vuelve al contenido de disco', async () => {
    const { tab } = await setup();
    await userEvent.click(fileBtn('invoice.ts'));
    await screen.findByText('4 bloques pendientes');
    await userEvent.click(screen.getByRole('button', { name: 'Todo suyo' }));
    expect(await screen.findByText('Sin bloques pendientes')).toBeInTheDocument();
    await userEvent.click(screen.getByRole('button', { name: 'Descartar mis ediciones' }));
    expect(await screen.findByText('4 bloques pendientes')).toBeInTheDocument();
    expect(conflictsOf(tab).dirtyCount).toBe(0);
  });
});
