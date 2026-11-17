//! IA en el formulario de crear pull request: propone título y descripción, y «Usar» los rellena.
//!
//! La IA solo propone: antes de «Usar» los campos siguen vacíos y no se crea ninguna PR.

import { cleanup, render, screen, waitFor, within } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { prOf } from '$lib/stores/pullRequests.svelte';
import { settingsUi } from '$lib/stores/settingsPanel.svelte';
import { resetGlobals } from '../../test/helpers';
import { prTab } from '../../test/prHelpers';
import PrDetail from './PrDetail.svelte';

beforeEach(resetGlobals);
afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
  settingsUi.close();
  resetGlobals();
});

describe('crear pull request con IA', () => {
  it('«Generar con IA» propone título y descripción, y «Usar» rellena los dos campos', async () => {
    const ctx = await prTab();
    render(PrDetail, { props: { tab: ctx.tab } });
    const pr = prOf(ctx.tab);
    pr.startCreate();

    const form = await screen.findByRole('form', { name: 'Nueva pull request' });
    const titulo = within(form).getByLabelText('Título') as HTMLInputElement;
    const descripcion = within(form).getByLabelText('Descripción') as HTMLTextAreaElement;
    // La descripción puede venir de la plantilla del repo; lo que importa es que no cambie sola.
    const antesTitulo = titulo.value;
    const antesDesc = descripcion.value;

    const generar = within(form).getByRole('button', { name: /Generar con IA/ });
    expect(generar).toBeEnabled();
    await userEvent.click(generar);

    const hoja = await screen.findByRole('dialog', { name: 'Propuesta de título y descripción de la PR' });
    // La IA solo propone: los campos del formulario siguen como estaban.
    expect(titulo.value).toBe(antesTitulo);
    expect(descripcion.value).toBe(antesDesc);

    // La propuesta trae título y cuerpo: no solo el asunto.
    const propuesto = (within(hoja).getByRole('textbox', { name: 'Título' }) as HTMLInputElement).value;
    const propuestoCuerpo = (within(hoja).getByRole('textbox', { name: 'Descripción' }) as HTMLTextAreaElement).value;
    expect(propuesto.trim().length).toBeGreaterThan(0);
    expect(propuestoCuerpo.trim().length).toBeGreaterThan(0);
    expect(propuestoCuerpo).not.toBe(antesDesc);

    await userEvent.click(within(hoja).getByRole('button', { name: /^Usar/ }));
    await waitFor(() => expect(titulo.value).toBe(propuesto));
    expect(descripcion.value).toBe(propuestoCuerpo);
    // La hoja se cierra al usar la propuesta.
    expect(screen.queryByRole('dialog', { name: 'Propuesta de título y descripción de la PR' })).toBeNull();
  });
});
