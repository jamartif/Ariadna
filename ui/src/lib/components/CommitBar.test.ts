import { render, screen, waitFor } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { installShortcuts } from '$lib/shortcuts/registry';
import { toasts } from '$lib/stores/toasts.svelte';
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

const box = () => screen.getByRole('textbox', { name: 'Mensaje' });

describe('CommitBar', () => {
  it('desactiva el botón sin mensaje y explica por qué', async () => {
    const { tab } = await loadedTab();
    render(CommitBar, { props: { tab } });
    const btn = screen.getByRole('button', { name: /Crear commit/ });
    expect(btn).toBeDisabled();
    expect(screen.getByText('Escribe un mensaje para poder confirmar.')).toBeInTheDocument();
    await userEvent.type(box(), 'Mensaje');
    expect(btn).toBeEnabled();
  });

  it('cuenta el asunto sobre 72 y avisa al pasarse', async () => {
    const { tab } = await loadedTab();
    render(CommitBar, { props: { tab } });
    await userEvent.type(box(), 'Hola');
    expect(screen.getByText('Asunto 4/72')).toBeInTheDocument();
    await userEvent.clear(box());
    await userEvent.click(box());
    await userEvent.paste('x'.repeat(73));
    expect(screen.getByText('Asunto 73/72')).toBeInTheDocument();
    expect(screen.getByText(/supera los 72 caracteres/)).toBeInTheDocument();
    expect(box()).toHaveAttribute('aria-invalid', 'true');
    // el contador solo mide la primera línea
    await userEvent.type(box(), '{Enter}descripción larga');
    expect(screen.getByText('Asunto 73/72')).toBeInTheDocument();
  });

  it('sin archivos preparados no se puede confirmar', async () => {
    const { tab } = await loadedTab();
    await tab.unstage(tab.sections.staged);
    await waitFor(() => expect(tab.stagedCount).toBe(0));
    render(CommitBar, { props: { tab } });
    await userEvent.type(box(), 'Algo');
    expect(screen.getByRole('button', { name: /Crear commit/ })).toBeDisabled();
    expect(screen.getByText('Prepara al menos un archivo para poder confirmar.')).toBeInTheDocument();
  });

  it('Ctrl+Intro crea el commit, vacía el borrador y limpia Preparados', async () => {
    const { tab, api } = await loadedTab();
    const spy = vi.spyOn(api, 'commit');
    render(CommitBar, { props: { tab } });
    await userEvent.type(box(), 'Añade el contrato IPC');
    await userEvent.keyboard('{Control>}{Enter}{/Control}');
    await waitFor(() => expect(spy).toHaveBeenCalledWith(tab.id, 'Añade el contrato IPC', false));
    await waitFor(() => expect(tab.message).toBe(''));
    await waitFor(() => expect(tab.stagedCount).toBe(0));
    expect(toasts.items.some((t) => t.kind === 'success' && t.title === 'Commit creado')).toBe(true);
  });

  it('Ctrl+Intro sin mensaje no hace nada', async () => {
    const { tab, api } = await loadedTab();
    const spy = vi.spyOn(api, 'commit');
    render(CommitBar, { props: { tab } });
    box().focus();
    await userEvent.keyboard('{Control>}{Enter}{/Control}');
    expect(spy).not.toHaveBeenCalled();
  });

  it('amend rellena el mensaje con amend_draft y lo restaura al desmarcar', async () => {
    const { tab, api } = await loadedTab();
    const spy = vi.spyOn(api, 'amendDraft');
    render(CommitBar, { props: { tab } });
    await userEvent.type(box(), 'borrador propio');
    const amend = screen.getByRole('checkbox', { name: /Corregir el último commit/ });
    await waitFor(() => expect(amend).toBeEnabled());
    await userEvent.click(amend);
    await waitFor(() => expect(tab.amend).toBe(true));
    expect(spy).toHaveBeenCalled();
    expect(box()).toHaveValue('Añade el motor de diff por hunk');
    expect(screen.getByRole('button', { name: /Corregir commit/ })).toBeEnabled();
    await userEvent.click(amend);
    await waitFor(() => expect(box()).toHaveValue('borrador propio'));
  });

  it('amend no disponible: casilla desactivada con el motivo', async () => {
    const { tab, api } = await loadedTab();
    api.amendDraft = async () => ({ message: '', canAmend: false, reason: 'No hay commits.' });
    render(CommitBar, { props: { tab } });
    await waitFor(() => expect(screen.getByRole('checkbox', { name: /Corregir el último commit/ })).toBeDisabled());
    expect(screen.getByText('No hay commits.')).toBeInTheDocument();
  });

  it('un error de commit se muestra como toast y conserva el mensaje', async () => {
    const { tab, api } = await loadedTab();
    api.commit = async () => {
      throw Object.assign(new Error('x'), { code: 'NOTHING_TO_COMMIT', message: 'No hay nada preparado.' });
    };
    render(CommitBar, { props: { tab } });
    await userEvent.type(box(), 'Intento');
    await userEvent.click(screen.getByRole('button', { name: /Crear commit/ }));
    await waitFor(() => expect(toasts.items.some((t) => t.kind === 'error' && t.title === 'No se pudo crear el commit')).toBe(true));
    expect(tab.message).toBe('Intento');
  });

  it('autoguarda el borrador por repositorio', async () => {
    const { tab } = await loadedTab();
    render(CommitBar, { props: { tab } });
    await userEvent.type(box(), 'a medias');
    expect(localStorage.getItem(`ariadna.draft:${tab.info.path}`)).toBe('a medias');
  });

  it('avisa si HEAD está desacoplado', async () => {
    const { tab } = await loadedTab();
    tab.snapshot = { ...tab.snapshot!, status: { ...tab.snapshot!.status, isDetached: true } };
    render(CommitBar, { props: { tab } });
    expect(screen.getByText(/HEAD no apunta a una rama/)).toBeInTheDocument();
  });
});
