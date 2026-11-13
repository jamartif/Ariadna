import { fireEvent, render, screen } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { installShortcuts } from '$lib/shortcuts/registry';
import type { Entry } from '$lib/stores/app.svelte';
import TabStrip from './TabStrip.svelte';

function repo(id: string, over: Partial<Extract<Entry, { kind: 'repo' }>['info']> = {}): Entry {
  return {
    id,
    kind: 'repo',
    info: { tabId: id, path: `/r/${id}`, name: id, branch: 'main', detached: false, changeCount: 0, conflictCount: 0, busy: null, missing: false, ...over }
  };
}

let off: () => void;
beforeEach(() => {
  off = installShortcuts(window, false);
});
afterEach(() => off());

function setup(entries: Entry[], activeId = 'a') {
  const props = {
    entries,
    activeId,
    onactivate: vi.fn(),
    onclose: vi.fn(),
    onnew: vi.fn(),
    onreorder: vi.fn()
  };
  render(TabStrip, { props });
  return props;
}

describe('TabStrip', () => {
  it('expone roles tablist/tab con aria-selected', () => {
    setup([repo('a'), repo('b'), repo('c')], 'b');
    expect(screen.getByRole('tablist', { name: 'Repositorios abiertos' })).toBeInTheDocument();
    const tabs = screen.getAllByRole('tab');
    expect(tabs).toHaveLength(3);
    expect(tabs.map((t) => t.getAttribute('aria-selected'))).toEqual(['false', 'true', 'false']);
    // roving tabindex: solo la activa entra en el orden de tabulación
    expect(tabs.map((t) => t.getAttribute('tabindex'))).toEqual(['-1', '0', '-1']);
  });

  it('las flechas mueven el foco sin activar; Intro activa', async () => {
    const p = setup([repo('a'), repo('b'), repo('c')], 'a');
    const [a, b, c] = screen.getAllByRole('tab') as HTMLElement[];
    a!.focus();
    await userEvent.keyboard('{ArrowRight}');
    expect(b).toHaveFocus();
    expect(p.onactivate).not.toHaveBeenCalled();
    await userEvent.keyboard('{End}');
    expect(c).toHaveFocus();
    await userEvent.keyboard('{ArrowRight}');
    expect(a).toHaveFocus(); // circular
    await userEvent.keyboard('{ArrowRight}{Enter}');
    expect(p.onactivate).toHaveBeenCalledWith('b');
  });

  it('Alt+Shift+→ reordena la pestaña enfocada', async () => {
    const p = setup([repo('a'), repo('b'), repo('c')], 'a');
    const [a] = screen.getAllByRole('tab') as HTMLElement[];
    a!.focus();
    await userEvent.keyboard('{Alt>}{Shift>}{ArrowRight}{/Shift}{/Alt}');
    expect(p.onreorder).toHaveBeenCalledWith(['b', 'a', 'c']);
  });

  it('Alt+Shift+← en la primera pestaña no cambia nada; en la última sí', async () => {
    const p = setup([repo('a'), repo('b'), repo('c')], 'a');
    const tabs = screen.getAllByRole('tab') as HTMLElement[];
    tabs[0]!.focus();
    await userEvent.keyboard('{Alt>}{Shift>}{ArrowLeft}{/Shift}{/Alt}');
    expect(p.onreorder).not.toHaveBeenCalled();
    tabs[2]!.focus();
    await fireEvent.keyDown(tabs[2]!, { key: 'ArrowLeft', altKey: true, shiftKey: true });
    expect(p.onreorder).toHaveBeenCalledWith(['a', 'c', 'b']);
  });

  it('el botón cerrar y la tecla Supr cierran; el botón + crea', async () => {
    const p = setup([repo('a'), repo('b')], 'a');
    await userEvent.click((() => {
      const tabs = [...document.querySelectorAll<HTMLElement>('.tab')];
      const b = tabs.find((x) => x.querySelector('.name')?.textContent === 'b');
      return b!.querySelector<HTMLElement>('.close')!;
    })());
    expect(p.onclose).toHaveBeenCalledWith('b');
    (screen.getAllByRole('tab')[0] as HTMLElement).focus();
    await userEvent.keyboard('{Delete}');
    expect(p.onclose).toHaveBeenCalledWith('a');
    await userEvent.click(screen.getByRole('button', { name: 'Nueva pestaña' }));
    expect(p.onnew).toHaveBeenCalledOnce();
  });

  it('indica rama, nº de cambios, conflicto, ocupado y missing', () => {
    setup(
      [
        repo('a', { changeCount: 7 }),
        repo('b', { conflictCount: 2, changeCount: 4 }),
        repo('c', { busy: 'Preparando 3 archivos' }),
        repo('d', { missing: true, branch: null })
      ],
      'a'
    );
    expect(screen.getByRole('tab', { name: /a main 7 rama main, 7 cambios/ })).toBeInTheDocument();
    expect(screen.getByRole('tab', { name: /2 conflictos/ })).toBeInTheDocument();
    expect(screen.getByRole('tab', { name: /ocupado: Preparando 3 archivos/ })).toBeInTheDocument();
    expect(screen.getByRole('tab', { name: /carpeta no encontrada/ })).toBeInTheDocument();
  });

  it('la pestaña «nueva» se rotula Nueva pestaña', () => {
    setup([repo('a'), { id: 'new:1', kind: 'new' }], 'new:1');
    expect(screen.getByRole('tab', { name: 'Nueva pestaña' })).toHaveAttribute('aria-selected', 'true');
  });

  it('avisa al acercarse al tope blando de 20 pestañas', () => {
    const many = Array.from({ length: 20 }, (_, i) => repo(`t${i}`));
    setup(many, 't0');
    expect(screen.getByText('20/20')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Nueva pestaña' })).toHaveAttribute('title', expect.stringContaining('20'));
  });

  it('el clic con botón central cierra', async () => {
    const p = setup([repo('a'), repo('b')], 'a');
    await fireEvent(screen.getAllByRole('tab')[1] as HTMLElement, new MouseEvent('auxclick', { button: 1, bubbles: true, cancelable: true }));
    expect(p.onclose).toHaveBeenCalledWith('b');
  });
});
