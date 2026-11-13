import { render, screen, waitFor, within } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { conflictsOf } from '$lib/stores/conflicts.svelte';
import { settingsUi } from '$lib/stores/settingsPanel.svelte';
import { loadedTab, resetGlobals } from '../../test/helpers';
import { expectNoSeriousViolations } from '../../test/axe';
import ConflictEditor from './ConflictEditor.svelte';
import Toasts from './Toasts.svelte';

/**
 * «Resolver con IA» en el estudio de conflictos: la propuesta es del archivo COMPLETO, se
 * revisa en un panel (diferencias contra el resultado actual) y solo al pulsar «Usar» pasa al
 * editor. Nunca se escribe en disco desde aquí.
 */

beforeEach(resetGlobals);
afterEach(() => {
  vi.restoreAllMocks();
  settingsUi.close();
  resetGlobals();
});

async function setup(opts: { scenario?: string; confirm?: () => boolean } = {}) {
  const ctx = await loadedTab('tab-legacy', {
    ...(opts.scenario ? { scenario: opts.scenario } : {}),
    ...(opts.confirm ? { confirm: opts.confirm } : {})
  });
  ctx.tab.panel = 'conflicts';
  // El estudio (ConflictsPanel) selecciona el primer archivo; aquí basta con sincronizar.
  const c = conflictsOf(ctx.tab);
  await c.sync();
  c.select(c.files[0]!);
  render(ConflictEditor, { props: { tab: ctx.tab } });
  render(Toasts);
  await waitFor(() => expect(conflictsOf(ctx.tab).selected).not.toBeNull());
  // AI: se espera al estado (decide entre botón y aviso con enlace a Ajustes).
  await waitFor(() => expect(ctx.tab.ai.state).not.toBeNull());
  return ctx;
}

const result = () => screen.getByRole('textbox', { name: /^Resultado editable/ }) as HTMLTextAreaElement;
const aiButton = () => screen.getByRole('button', { name: /Resolver con IA/ });
const dialog = () => screen.getByRole('dialog', { name: 'Resolución propuesta con IA' });

describe('«Resolver con IA» en el editor de conflictos', () => {
  it('el botón está visible, con su atajo, y pide la propuesta sin tocar el archivo', async () => {
    const { tab } = await setup();
    const btn = aiButton();
    expect(btn).toBeEnabled();
    expect(btn.textContent).toMatch(/Alt\+R/);
    expect(result().value).toContain('<<<<<<< HEAD');

    await userEvent.click(btn);
    await screen.findByRole('dialog', { name: 'Resolución propuesta con IA' });
    expect(screen.getByText('Por qué así')).toBeInTheDocument();
    expect(screen.getByText(/Se conservan los dos cambios/)).toBeInTheDocument();
    // El editor sigue con los marcadores: la propuesta NO se ha aplicado.
    expect(result().value).toContain('<<<<<<< HEAD');
    expect(conflictsOf(tab).isDirty(conflictsOf(tab).selected!)).toBe(false);
    // Y en Rust tampoco se ha resuelto nada (siguen los 4 archivos en conflicto).
    expect((await tab.api.mergeState(tab.id)).conflicts).toHaveLength(4);
  });

  it('muestra las diferencias de la propuesta contra el resultado actual', async () => {
    await setup();
    await userEvent.click(aiButton());
    const dlg = await screen.findByRole('dialog', { name: 'Resolución propuesta con IA' });
    const diff = within(dlg).getByRole('list', { name: 'Diferencias de la propuesta contra el resultado actual' });
    const rows = within(diff).getAllByRole('listitem');
    expect(rows.length).toBeGreaterThan(0);
    expect(within(dlg).getByText('diferencias contra el resultado actual')).toBeInTheDocument();
    // Las líneas que quita (los marcadores) llevan su signo; las que añade, el suyo.
    expect(within(dlg).getByText(/^\+\d+$/)).toBeInTheDocument();
    expect(within(dlg).getByText(/^−\d+$/)).toBeInTheDocument();
  });

  it('«Usar» pone la propuesta en el editor como texto editable y actualiza los bloques', async () => {
    const { tab } = await setup();
    await screen.findByText('4 bloques pendientes');
    await userEvent.click(aiButton());
    const dlg = await screen.findByRole('dialog', { name: 'Resolución propuesta con IA' });
    await userEvent.click(within(dlg).getByRole('button', { name: 'Usar' }));

    await waitFor(() => expect(screen.queryByRole('dialog', { name: 'Resolución propuesta con IA' })).not.toBeInTheDocument());
    expect(result().value).not.toMatch(/<<<<<<<|=======|>>>>>>>/);
    expect(await screen.findByText('Sin bloques pendientes')).toBeInTheDocument();
    // Es un borrador: «Marcar como resuelto» se habilita, pero nada se ha escrito aún.
    expect(screen.getByRole('button', { name: 'Marcar como resuelto' })).toBeEnabled();
    expect(conflictsOf(tab).isDirty(conflictsOf(tab).selected!)).toBe(true);
    expect((await tab.api.mergeState(tab.id)).conflicts).toHaveLength(4);
  });

  it('«Regenerar» pide otra propuesta y mantiene el panel abierto', async () => {
    const { api } = await setup();
    const spy = vi.spyOn(api, 'aiResolveConflict');
    await userEvent.click(aiButton());
    const dlg = await screen.findByRole('dialog', { name: 'Resolución propuesta con IA' });
    expect(spy).toHaveBeenCalledTimes(1);
    await userEvent.click(within(dlg).getByRole('button', { name: 'Regenerar' }));
    await waitFor(() => expect(spy).toHaveBeenCalledTimes(2));
    expect(screen.getByRole('dialog', { name: 'Resolución propuesta con IA' })).toBeInTheDocument();
  });

  it('«Descartar» cierra el panel y deja el resultado como estaba', async () => {
    await setup();
    const before = result().value;
    await userEvent.click(aiButton());
    const dlg = await screen.findByRole('dialog', { name: 'Resolución propuesta con IA' });
    await userEvent.click(within(dlg).getByRole('button', { name: 'Descartar' }));
    await waitFor(() => expect(screen.queryByRole('dialog', { name: 'Resolución propuesta con IA' })).not.toBeInTheDocument());
    expect(result().value).toBe(before);
  });

  it('estado de carga con cancelar; cancelar mata la generación', async () => {
    const { tab } = await setup({ scenario: 'ai-slow' });
    await userEvent.click(aiButton());
    await screen.findByText('Resolviendo el conflicto con IA…');
    const cancel = screen.getByRole('button', { name: /Cancelar/ });
    await userEvent.click(cancel);
    await waitFor(() => expect(screen.queryByText('Resolviendo el conflicto con IA…')).not.toBeInTheDocument());
    expect(aiButton()).toBeEnabled();
    expect(screen.queryByRole('dialog', { name: 'Resolución propuesta con IA' })).not.toBeInTheDocument();
    // Nada escrito en disco: el conflicto sigue ahí.
    expect((await tab.api.mergeState(tab.id)).conflicts).toHaveLength(4);
  });

  it('una propuesta inválida (con marcadores) se explica con un error y se puede reintentar', async () => {
    await setup({ scenario: 'ai-conflict-markers' });
    await userEvent.click(aiButton());
    expect(await screen.findByRole('alert')).toHaveTextContent(/no sirve como resolución/);
    expect(screen.queryByRole('dialog', { name: 'Resolución propuesta con IA' })).not.toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Reintentar' })).toBeInTheDocument();
    expect(result().value).toContain('<<<<<<< HEAD');
  });

  it('sin consentimiento no se envía nada y se ofrece Ajustes', async () => {
    const { api } = await setup({ confirm: () => false });
    const spy = vi.spyOn(api, 'aiResolveConflict');
    await userEvent.click(aiButton());
    expect(await screen.findByRole('alert')).toHaveTextContent(/Sin tu permiso/);
    expect(spy).toHaveBeenCalledTimes(1);
    await userEvent.click(screen.getByRole('button', { name: 'Abrir Ajustes de IA' }));
    expect(settingsUi.isOpen).toBe(true);
    expect(settingsUi.section).toBe('ai');
  });

  it('con la IA desactivada avisa y enlaza a Ajustes, y el botón no hace nada', async () => {
    const { api } = await setup({ scenario: 'ai-disabled' });
    const spy = vi.spyOn(api, 'aiResolveConflict');
    expect(await screen.findByText('La IA está desactivada.')).toBeInTheDocument();
    expect(aiButton()).toBeDisabled();
    await userEvent.click(screen.getByRole('button', { name: 'Abrir Ajustes de IA' }));
    expect(settingsUi.section).toBe('ai');
    expect(spy).not.toHaveBeenCalled();
  });

  it('el panel con la propuesta no tiene violaciones graves de accesibilidad', async () => {
    await setup();
    await userEvent.click(aiButton());
    await screen.findByRole('dialog', { name: 'Resolución propuesta con IA' });
    await expectNoSeriousViolations();
  });
});
