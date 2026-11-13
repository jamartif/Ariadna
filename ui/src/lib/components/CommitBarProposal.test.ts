import { render, screen, waitFor, within } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { AiCommitSuggestion } from '$lib/api/types';
import { installShortcuts } from '$lib/shortcuts/registry';
import { loadedTab, resetGlobals } from '../../test/helpers';
import CommitBar from './CommitBar.svelte';

let off: () => void;
beforeEach(() => {
  resetGlobals();
  off = installShortcuts(window, false);
});
afterEach(() => {
  off();
  resetGlobals();
});

// La hoja repite las etiquetas «Asunto» y «Descripción»: los de la barra son los primeros del DOM.
const subject = () => screen.getAllByRole('textbox', { name: 'Asunto' })[0] as HTMLInputElement;
const body = () => screen.getAllByRole('textbox', { name: 'Descripción' })[0] as HTMLTextAreaElement;
const sheet = () => screen.getByRole('dialog', { name: 'Propuesta de mensaje de commit' });
const sug = (subject: string, text = 'Detalle'): AiCommitSuggestion => ({ subject, body: text, provider: 'claude', commandSummary: 'claude -p --output-format json (stdin)', repoRevision: 1 });

async function generated() {
  const { tab, api } = await loadedTab('tab-ariadna');
  render(CommitBar, { props: { tab } });
  await waitFor(() => expect(tab.ai.state).not.toBeNull());
  await userEvent.type(subject(), 'Texto de antes');
  const next = vi.spyOn(api, 'aiGenerateCommitMessage');
  next.mockResolvedValue(sug('Primera propuesta', '- uno'));
  await userEvent.click(screen.getByRole('button', { name: /Generar con IA/ }));
  await waitFor(() => expect(subject()).toHaveValue('Primera propuesta'));
  return { tab, api, next };
}

describe('CommitBar · hoja de propuesta', () => {
  it('«Ver propuesta» abre la hoja con proveedor, perfil y cuenta, sin tocar los campos ni hacer commit', async () => {
    const { api } = await generated();
    const commit = vi.spyOn(api, 'commit');
    await userEvent.click(screen.getByRole('button', { name: 'Ver propuesta' }));
    const dlg = sheet();
    expect(within(dlg).getByRole('textbox', { name: 'Asunto' })).toHaveValue('Primera propuesta');
    expect(within(dlg).getByText('Propuesto por Claude')).toBeInTheDocument();
    expect(within(dlg).getByText('Perfil Personal')).toBeInTheDocument();
    await waitFor(() => expect(within(dlg).getByText('Cuenta: javier@example.com')).toBeInTheDocument());
    expect(commit).not.toHaveBeenCalled();
    expect(subject()).toHaveValue('Primera propuesta');
  });

  it('Usar escribe lo editado en la hoja y cierra; sigue disponible Deshacer', async () => {
    const { tab, api } = await generated();
    const commit = vi.spyOn(api, 'commit');
    await userEvent.click(screen.getByRole('button', { name: 'Ver propuesta' }));
    const field = within(sheet()).getByRole('textbox', { name: 'Asunto' });
    await userEvent.clear(field);
    await userEvent.type(field, 'Asunto retocado');
    await userEvent.click(within(sheet()).getByRole('button', { name: 'Usar' }));
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
    expect(subject()).toHaveValue('Asunto retocado');
    expect(body()).toHaveValue('- uno');
    expect(tab.summary).toBe('Asunto retocado');
    expect(commit).not.toHaveBeenCalled();

    await userEvent.click(screen.getByRole('button', { name: 'Deshacer' }));
    expect(subject()).toHaveValue('Texto de antes');
  });

  it('Descartar restaura el texto de antes y quita el aviso', async () => {
    await generated();
    await userEvent.click(screen.getByRole('button', { name: 'Ver propuesta' }));
    await userEvent.click(within(sheet()).getByRole('button', { name: 'Descartar' }));
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
    expect(subject()).toHaveValue('Texto de antes');
    expect(screen.queryByText(/Propuesto por/)).not.toBeInTheDocument();
  });

  it('Regenerar pide otra propuesta, mantiene la hoja abierta y actualiza los campos si seguían igual', async () => {
    const { next, tab } = await generated();
    await userEvent.click(screen.getByRole('button', { name: 'Ver propuesta' }));
    let release!: (s: AiCommitSuggestion) => void;
    next.mockImplementation(() => new Promise((r) => (release = r)));
    await userEvent.click(within(sheet()).getByRole('button', { name: 'Regenerar' }));
    expect(within(sheet()).getByRole('button', { name: /Regenerando/ })).toBeDisabled();
    release(sug('Segunda propuesta', '- dos'));
    await waitFor(() => expect(within(sheet()).getByRole('textbox', { name: 'Asunto' })).toHaveValue('Segunda propuesta'));
    expect(subject()).toHaveValue('Segunda propuesta');
    expect(next).toHaveBeenCalledTimes(2);
    // Descartar sigue devolviendo lo de ANTES de la IA, no la primera propuesta.
    await userEvent.click(within(sheet()).getByRole('button', { name: 'Descartar' }));
    expect(tab.summary).toBe('Texto de antes');
  });

  it('Regenerar no pisa lo que el usuario ya cambió en los campos', async () => {
    const { next } = await generated();
    await userEvent.type(subject(), ' (a mano)');
    await userEvent.click(screen.getByRole('button', { name: 'Ver propuesta' }));
    next.mockResolvedValue(sug('Segunda propuesta', '- dos'));
    await userEvent.click(within(sheet()).getByRole('button', { name: 'Regenerar' }));
    await waitFor(() => expect(within(sheet()).getByRole('textbox', { name: 'Asunto' })).toHaveValue('Segunda propuesta'));
    expect(subject()).toHaveValue('Primera propuesta (a mano)');
    await userEvent.click(within(sheet()).getByRole('button', { name: 'Usar' }));
    expect(subject()).toHaveValue('Segunda propuesta');
  });

  it('si regenerar falla, la hoja enseña el error y conserva la propuesta', async () => {
    const { next } = await generated();
    await userEvent.click(screen.getByRole('button', { name: 'Ver propuesta' }));
    next.mockRejectedValue(Object.assign(new Error('x'), { code: 'AI_QUOTA', message: 'Claude ha agotado su cuota.' }));
    await userEvent.click(within(sheet()).getByRole('button', { name: 'Regenerar' }));
    expect(await within(sheet()).findByRole('alert')).toHaveTextContent('Claude ha agotado su cuota.');
    expect(within(sheet()).getByRole('textbox', { name: 'Asunto' })).toHaveValue('Primera propuesta');
    expect(within(sheet()).getByRole('button', { name: 'Regenerar' })).toBeEnabled();
  });

  it('Esc cierra la hoja sin cambiar nada', async () => {
    await generated();
    await userEvent.click(screen.getByRole('button', { name: 'Ver propuesta' }));
    await userEvent.keyboard('{Escape}');
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
    expect(subject()).toHaveValue('Primera propuesta');
  });

  it('sin la cuenta (CLI que no la expone) la hoja dice que no está disponible', async () => {
    const { tab, api } = await generated();
    vi.spyOn(api, 'aiAccounts').mockRejectedValue(new Error('x'));
    tab.ai.accounts = null;
    await userEvent.click(screen.getByRole('button', { name: 'Ver propuesta' }));
    expect(within(sheet()).getByText('Cuenta no disponible')).toBeInTheDocument();
  });
});
