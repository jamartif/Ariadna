import { render, screen, within } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { toasts } from '$lib/stores/toasts.svelte';
import { resetGlobals } from '../../test/helpers';
import ProposalSheet from './ProposalSheet.svelte';

beforeEach(resetGlobals);
afterEach(resetGlobals);

function setup(props: Record<string, unknown> = {}) {
  const cb = { onuse: vi.fn(), onregenerate: vi.fn(), ondiscard: vi.fn(), onclose: vi.fn() };
  render(ProposalSheet, {
    props: { kind: 'commit', title: 'Añade el saludo', body: '- Crea saludo.txt', providerName: 'Claude', profileName: 'Empresa', account: 'ana@empresa.com · Empresa SL', ...cb, ...props }
  });
  return cb;
}

const clip = () => {
  const write = vi.fn().mockResolvedValue(undefined);
  Object.defineProperty(navigator, 'clipboard', { value: { writeText: write }, configurable: true });
  return write;
};

describe('ProposalSheet', () => {
  it('es un diálogo modal que dice quién propone, con qué perfil y con qué cuenta', () => {
    setup();
    const dlg = screen.getByRole('dialog', { name: 'Propuesta de mensaje de commit' });
    expect(dlg).toHaveAttribute('aria-modal', 'true');
    expect(within(dlg).getByText('Propuesto por Claude')).toBeInTheDocument();
    expect(within(dlg).getByText('Perfil Empresa')).toBeInTheDocument();
    expect(within(dlg).getByText('Cuenta: ana@empresa.com · Empresa SL')).toBeInTheDocument();
    expect(screen.getByRole('textbox', { name: 'Asunto' })).toHaveValue('Añade el saludo');
    expect(screen.getByRole('textbox', { name: 'Descripción' })).toHaveValue('- Crea saludo.txt');
  });

  it('sin cuenta que mostrar dice «no disponible» en vez de inventarla', () => {
    setup({ account: null });
    expect(screen.getByText('Cuenta no disponible')).toBeInTheDocument();
  });

  it('para una PR usa «Título» y su propio encabezado', () => {
    setup({ kind: 'pr', title: 'Añade acceso' });
    expect(screen.getByRole('dialog', { name: 'Propuesta de título y descripción de la PR' })).toBeInTheDocument();
    expect(screen.getByRole('textbox', { name: 'Título' })).toHaveValue('Añade acceso');
  });

  it('Copiar copia exactamente lo que se ve, incluido lo editado', async () => {
    setup();
    const write = clip();
    await userEvent.click(screen.getByRole('button', { name: /Copiar/ }));
    expect(write).toHaveBeenLastCalledWith('Añade el saludo\n\n- Crea saludo.txt');
    await userEvent.clear(screen.getByRole('textbox', { name: 'Asunto' }));
    await userEvent.type(screen.getByRole('textbox', { name: 'Asunto' }), 'Otro asunto');
    await userEvent.click(screen.getByRole('button', { name: /Copiar/ }));
    expect(write).toHaveBeenLastCalledWith('Otro asunto\n\n- Crea saludo.txt');
    expect(toasts.items.at(-1)).toMatchObject({ kind: 'success', title: 'Propuesta copiada al portapapeles' });
  });

  it('si el portapapeles falla avisa sin romper la hoja', async () => {
    setup();
    Object.defineProperty(navigator, 'clipboard', { value: { writeText: vi.fn().mockRejectedValue(new Error('no')) }, configurable: true });
    await userEvent.click(screen.getByRole('button', { name: /Copiar/ }));
    await vi.waitFor(() => expect(toasts.items.at(-1)).toMatchObject({ kind: 'warning' }));
    expect(toasts.items.at(-1)?.title).toMatch(/No se pudo copiar/);
    expect(screen.getByRole('dialog')).toBeInTheDocument();
  });

  it('Usar devuelve el texto editado y limpio; con título vacío queda inactivo', async () => {
    const cb = setup();
    await userEvent.type(screen.getByRole('textbox', { name: 'Descripción' }), '\n- Extra  ');
    await userEvent.click(screen.getByRole('button', { name: 'Usar' }));
    expect(cb.onuse).toHaveBeenCalledWith({ title: 'Añade el saludo', body: '- Crea saludo.txt\n- Extra' });

    await userEvent.clear(screen.getByRole('textbox', { name: 'Asunto' }));
    expect(screen.getByRole('button', { name: 'Usar' })).toBeDisabled();
    expect(screen.getByRole('button', { name: /Copiar/ })).toBeDisabled();
    expect(screen.getByRole('alert')).toHaveTextContent('no puede quedarse sin título');
  });

  it('Regenerar, Descartar y Esc avisan al que la usa', async () => {
    const cb = setup();
    await userEvent.click(screen.getByRole('button', { name: 'Regenerar' }));
    expect(cb.onregenerate).toHaveBeenCalledTimes(1);
    await userEvent.click(screen.getByRole('button', { name: 'Descartar' }));
    expect(cb.ondiscard).toHaveBeenCalledTimes(1);
    await userEvent.keyboard('{Escape}');
    expect(cb.onclose).toHaveBeenCalledTimes(1);
    await userEvent.click(screen.getByRole('button', { name: 'Cerrar' }));
    expect(cb.onclose).toHaveBeenCalledTimes(2);
    expect(cb.onuse).not.toHaveBeenCalled();
  });

  it('regenerando bloquea los campos y las acciones que dependen del texto', () => {
    setup({ regenerating: true });
    expect(screen.getByRole('button', { name: /Regenerando/ })).toBeDisabled();
    expect(screen.getByRole('button', { name: 'Usar' })).toBeDisabled();
    expect(screen.getByRole('button', { name: 'Descartar' })).toBeDisabled();
    expect(screen.getByRole('textbox', { name: 'Asunto' })).toBeDisabled();
  });

  it('muestra obsoleta, el error de la regeneración y que ya está en uso', () => {
    setup({ stale: true, error: 'Claude no respondió', inUse: true });
    expect(screen.getByText(/puede estar obsoleta/)).toBeInTheDocument();
    expect(screen.getByRole('alert')).toHaveTextContent('Claude no respondió');
    expect(screen.getByText('Esta propuesta ya está en los campos.')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Usar de nuevo' })).toBeInTheDocument();
  });

  it('una propuesta nueva sustituye lo editado', async () => {
    const { rerender } = render(ProposalSheet, {
      props: { kind: 'commit', title: 'Uno', body: 'a', providerName: 'Claude', onuse: vi.fn(), onregenerate: vi.fn(), ondiscard: vi.fn(), onclose: vi.fn() }
    });
    await userEvent.type(screen.getByRole('textbox', { name: 'Asunto' }), ' editado');
    await rerender({ title: 'Dos', body: 'b' });
    expect(screen.getByRole('textbox', { name: 'Asunto' })).toHaveValue('Dos');
    expect(screen.getByRole('textbox', { name: 'Descripción' })).toHaveValue('b');
  });
});
