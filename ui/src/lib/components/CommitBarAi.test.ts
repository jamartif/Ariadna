import { render, screen, waitFor, within } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { AiCommitSuggestion } from '$lib/api/types';
import { installShortcuts, runShortcut } from '$lib/shortcuts/registry';
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

const subject = () => screen.getByRole('textbox', { name: 'Asunto' }) as HTMLInputElement;
const body = () => screen.getByRole('textbox', { name: 'Descripción' }) as HTMLTextAreaElement;
const aiButton = () => screen.getByRole('button', { name: /Generar con IA/ });

/** Pestaña con la barra montada y el estado de IA ya cargado. */
async function setup(scenario?: string) {
  const { tab, api } = await loadedTab('tab-ariadna', scenario ? { scenario } : {});
  render(CommitBar, { props: { tab } });
  await waitFor(() => expect(tab.ai.state).not.toBeNull());
  return { tab, api };
}

describe('CommitBar · generar con IA', () => {
  it('vacío: sin nada preparado el botón queda inactivo, explica por qué y no llama a la IA', async () => {
    const { tab, api } = await setup();
    const spy = vi.spyOn(api, 'aiGenerateCommitMessage');
    await tab.unstage(tab.sections.staged);
    await waitFor(() => expect(tab.stagedCount).toBe(0));

    await waitFor(() => expect(aiButton()).toHaveAttribute('aria-disabled', 'true'));
    expect(aiButton()).toHaveAttribute('title', 'Prepara cambios para que la IA pueda describirlos.');
    expect(aiButton()).toHaveAccessibleDescription('Prepara cambios para que la IA pueda describirlos.');
    await userEvent.click(aiButton());
    expect(spy).not.toHaveBeenCalled();
    expect(subject()).toHaveValue('');
  });

  it('con cambios preparados el botón muestra el atajo y está activo', async () => {
    await setup();
    expect(aiButton()).not.toHaveAttribute('aria-disabled');
    expect(aiButton().getAttribute('title')).toMatch(/Ctrl\+Shift\+G|Ctrl\+Mayús\+G|Ctrl\+⇧\+G|⌘/);
  });

  it('cargando: spinner, botón Cancelar y campos bloqueados hasta que responde', async () => {
    const { tab, api } = await setup();
    let release!: (s: AiCommitSuggestion) => void;
    vi.spyOn(api, 'aiGenerateCommitMessage').mockImplementation(() => new Promise((r) => (release = r)));

    await userEvent.click(aiButton());
    expect(await screen.findByText('Generando…')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /Cancelar/ })).toBeInTheDocument();
    expect(subject()).toHaveAttribute('readonly');
    expect(body()).toHaveAttribute('readonly');
    expect(screen.getByRole('button', { name: /Crear commit/ })).toBeDisabled();
    expect(screen.queryByRole('button', { name: /Generar con IA/ })).not.toBeInTheDocument();

    release({ subject: 'Añade X', body: 'Detalle', provider: 'claude', commandSummary: 'claude -p', repoRevision: tab.revision });
    await waitFor(() => expect(subject()).toHaveValue('Añade X'));
    expect(screen.queryByText('Generando…')).not.toBeInTheDocument();
    expect(subject()).not.toHaveAttribute('readonly');
  });

  it('éxito: rellena asunto y descripción, dice qué proveedor y NO crea el commit', async () => {
    const { tab, api } = await setup();
    const commit = vi.spyOn(api, 'commit');
    const amend = vi.spyOn(api, 'amend');

    await userEvent.click(aiButton());
    await waitFor(() => expect(subject().value).not.toBe(''));
    expect(subject().value.length).toBeLessThanOrEqual(72);
    expect(body().value).not.toBe('');
    expect(tab.message).toBe(`${tab.summary}\n\n${tab.description}`);
    expect(screen.getByText(/Propuesto por Claude/)).toBeInTheDocument();
    expect(commit).not.toHaveBeenCalled();
    expect(amend).not.toHaveBeenCalled();
    // El usuario sigue decidiendo: el botón de commit se habilita pero no se pulsa solo.
    expect(screen.getByRole('button', { name: /Crear commit/ })).toBeEnabled();
  });

  it('la IA no envía el diff desde la interfaz: solo tabId y opciones', async () => {
    const { tab, api } = await setup();
    const spy = vi.spyOn(api, 'aiGenerateCommitMessage');
    await userEvent.click(aiButton());
    await waitFor(() => expect(spy).toHaveBeenCalledTimes(1));
    expect(spy).toHaveBeenCalledWith(tab.id, { template: null, language: 'auto', style: 'plain' });
  });

  it('error del proveedor: aviso con el mensaje, reintentar, y no toca lo escrito', async () => {
    const { api } = await setup('ai-quota');
    await userEvent.type(subject(), 'Mi texto');
    await userEvent.click(aiButton());

    const alert = await screen.findByRole('alert');
    expect(within(alert).getByText('No se pudo generar el mensaje')).toBeInTheDocument();
    expect(within(alert).getByText(/límite de uso/)).toBeInTheDocument();
    expect(subject()).toHaveValue('Mi texto');
    expect(within(alert).getByRole('button', { name: 'Reintentar' })).toBeInTheDocument();

    // Reintentar vuelve a llamar.
    const spy = vi.spyOn(api, 'aiGenerateCommitMessage');
    await userEvent.click(within(alert).getByRole('button', { name: 'Reintentar' }));
    await waitFor(() => expect(spy).toHaveBeenCalledTimes(1));
    // Descartar el aviso.
    await userEvent.click((await screen.findByRole('alert')).querySelector('button[aria-label="Descartar aviso"]') as HTMLElement);
    await waitFor(() => expect(screen.queryByRole('alert')).not.toBeInTheDocument());
  });

  it('sin sesión: indica el comando que hay que ejecutar y ofrece abrir los ajustes', async () => {
    await setup('ai-not-logged');
    await userEvent.click(aiButton());
    const alert = await screen.findByRole('alert');
    expect(within(alert).getByText(/Ejecuta en una terminal: claude/)).toBeInTheDocument();
    await userEvent.click(within(alert).getByRole('button', { name: 'Ajustes de IA' }));
    expect(await screen.findByRole('dialog', { name: /IA para el mensaje de commit/ })).toBeInTheDocument();
  });

  it('sin consentimiento: el error remite a los ajustes y no se rellena nada', async () => {
    await setup('ai-consent-denied');
    await userEvent.click(aiButton());
    const alert = await screen.findByRole('alert');
    expect(within(alert).getByRole('button', { name: 'Ajustes de IA' })).toBeInTheDocument();
    expect(subject()).toHaveValue('');
  });

  it('deshacer: restaura el texto que había antes de rellenar', async () => {
    const { tab } = await setup();
    await userEvent.type(subject(), 'Texto mío');
    await userEvent.type(body(), 'Mi descripción');

    await userEvent.click(aiButton());
    await waitFor(() => expect(subject().value).not.toBe('Texto mío'));
    await userEvent.click(screen.getByRole('button', { name: 'Deshacer' }));

    expect(subject()).toHaveValue('Texto mío');
    expect(body()).toHaveValue('Mi descripción');
    expect(screen.queryByRole('button', { name: 'Deshacer' })).not.toBeInTheDocument();
    expect(tab.ai.notice).toBeNull();
  });

  it('cancelar: vuelve el botón, llama a ai_cancel y no rellena nada', async () => {
    const { api } = await setup('ai-slow');
    const cancel = vi.spyOn(api, 'aiCancel');
    await userEvent.type(subject(), 'Intacto');

    await userEvent.click(aiButton());
    await userEvent.click(await screen.findByRole('button', { name: /Cancelar/ }));

    await waitFor(() => expect(aiButton()).toBeInTheDocument());
    expect(cancel).toHaveBeenCalledTimes(1);
    expect(subject()).toHaveValue('Intacto');
    expect(subject()).not.toHaveAttribute('readonly');
    // Cancelar no es un error: no hay aviso rojo.
    expect(screen.queryByRole('alert')).not.toBeInTheDocument();
  });

  it('Escape también cancela la generación', async () => {
    const { api } = await setup('ai-slow');
    const cancel = vi.spyOn(api, 'aiCancel');
    await userEvent.click(aiButton());
    await screen.findByRole('button', { name: /Cancelar/ });
    await userEvent.keyboard('{Escape}');
    await waitFor(() => expect(cancel).toHaveBeenCalled());
    await waitFor(() => expect(aiButton()).toBeInTheDocument());
  });

  it('el atajo Ctrl+Mayús+G y la paleta lanzan la generación', async () => {
    const { api } = await setup();
    const spy = vi.spyOn(api, 'aiGenerateCommitMessage');
    await userEvent.keyboard('{Control>}{Shift>}g{/Shift}{/Control}');
    await waitFor(() => expect(spy).toHaveBeenCalledTimes(1));
    await waitFor(() => expect(subject().value).not.toBe(''));

    expect(runShortcut('ai.generate')).toBe(true);
    await waitFor(() => expect(spy).toHaveBeenCalledTimes(2));
  });

  it('IA desactivada: avisa, abre el diálogo de consentimiento y al activarla genera', async () => {
    const { tab, api } = await setup('ai-disabled');
    expect(screen.getByText('La IA está desactivada')).toBeInTheDocument();
    const spy = vi.spyOn(api, 'aiGenerateCommitMessage');

    await userEvent.click(aiButton());
    expect(spy).not.toHaveBeenCalled();
    const dialog = await screen.findByRole('dialog', { name: /IA para el mensaje de commit/ });
    // Explica qué se envía y a quién.
    expect(within(dialog).getByText(/diff de los archivos preparados/)).toBeInTheDocument();
    expect(within(dialog).getByText(/Nunca se envían \.env/)).toBeInTheDocument();
    expect(within(dialog).getByText(/solo propone/)).toBeInTheDocument();

    await userEvent.click(within(dialog).getByRole('radio', { name: /Empresa/ }));
    await userEvent.click(within(dialog).getByRole('radio', { name: /Codex/ }));
    await userEvent.click(within(dialog).getByRole('button', { name: 'Activar IA' }));

    await waitFor(() => expect(screen.queryByRole('dialog')).not.toBeInTheDocument());
    expect(tab.ai.state?.enabled).toBe(true);
    expect(tab.ai.state?.profile.kind).toBe('empresa');
    expect(tab.ai.state?.choice).toBe('codex');
    expect(screen.queryByText('La IA está desactivada')).not.toBeInTheDocument();
  });

  it('el diálogo lista los cuatro modos y se cierra con Escape', async () => {
    await setup('ai-not-logged');
    await userEvent.click(aiButton());
    await userEvent.click(await screen.findByRole('button', { name: 'Ajustes de IA' }));
    const dialog = await screen.findByRole('dialog');
    for (const name of [/Automático/, /Claude/, /Codex/, /Copilot/]) {
      expect(within(dialog).getByRole('radio', { name })).toBeInTheDocument();
    }
    expect(within(dialog).getByRole('button', { name: 'Desactivar IA' })).toBeInTheDocument();
    await userEvent.keyboard('{Escape}');
    await waitFor(() => expect(screen.queryByRole('dialog')).not.toBeInTheDocument());
  });

  it('desactivar la IA desde el diálogo muestra el aviso de IA desactivada', async () => {
    const { tab } = await setup('ai-not-logged');
    await userEvent.click(aiButton());
    await userEvent.click(await screen.findByRole('button', { name: 'Ajustes de IA' }));
    const dialog = await screen.findByRole('dialog');
    await userEvent.click(within(dialog).getByRole('button', { name: 'Desactivar IA' }));
    await waitFor(() => expect(tab.ai.state?.enabled).toBe(false));
    expect(await screen.findByText('La IA está desactivada')).toBeInTheDocument();
  });

  it('la propuesta se marca como obsoleta si el repositorio cambió mientras la IA pensaba', async () => {
    const { tab, api } = await setup();
    vi.spyOn(api, 'aiGenerateCommitMessage').mockImplementation(async (id) => ({
      subject: 'Añade X',
      body: '',
      provider: 'claude',
      commandSummary: 'claude -p',
      repoRevision: tab.revision - 1 // Rust leyó una revisión anterior
    }));
    await userEvent.click(aiButton());
    expect(await screen.findByText(/puede estar obsoleta/)).toBeInTheDocument();
  });

  it('crear el commit limpia el aviso de la IA', async () => {
    const { tab } = await setup();
    await userEvent.click(aiButton());
    await screen.findByText(/Propuesto por Claude/);
    await userEvent.click(screen.getByRole('button', { name: /Crear commit/ }));
    await waitFor(() => expect(tab.message).toBe(''));
    expect(screen.queryByText(/Propuesto por Claude/)).not.toBeInTheDocument();
  });
});
