import { fireEvent, render, screen } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { installShortcuts } from '$lib/shortcuts/registry';
import type { Entry } from '$lib/stores/app.svelte';
import TabStrip from './TabStrip.svelte';

/** Fuente del componente: sirve para comprobar el contrato de maquetación de la tira. */
const FUENTE = readFileSync(join(process.cwd(), 'src/lib/components/TabStrip.svelte'), 'utf8');

/** Bloque CSS de un selector (`  .scroller {` … `}`). */
function regla(selector: string): string {
  const i = FUENTE.indexOf(`  ${selector} {`);
  expect(i, `no encuentro la regla ${selector}`).toBeGreaterThan(-1);
  return FUENTE.slice(i, FUENTE.indexOf('\n  }', i));
}

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

describe('TabStrip · ancho', () => {
  it('la pestaña se ajusta al nombre: no hay tirador para cambiar su ancho', () => {
    setup([repo('a'), repo('b')]);
    expect(screen.queryAllByTestId('tab-resize')).toHaveLength(0);
    for (const el of document.querySelectorAll<HTMLElement>('.tab')) expect(el.style.width).toBe('');
  });
});

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
    expect(screen.getByRole('tab', { name: /a 7 rama main, 7 cambios/ })).toBeInTheDocument();
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

    it('con muchas pestañas la rueda desplaza la tira para llegar a las de la derecha', async () => {
      const many = Array.from({ length: 30 }, (_, i) => repo(`t${i}`));
      setup(many, 't0');
      const scroller = screen.getByTestId('tabs-scroller');
      // jsdom no mide: se simula que la tira desborda y que el desplazamiento es escribible.
      let left = 0;
      Object.defineProperty(scroller, 'scrollWidth', { configurable: true, value: 3000 });
      Object.defineProperty(scroller, 'clientWidth', { configurable: true, value: 800 });
      Object.defineProperty(scroller, 'scrollLeft', {
        configurable: true,
        get: () => left,
        set: (v: number) => {
          left = v;
        }
      });

      await fireEvent(scroller, new WheelEvent('wheel', { deltaY: 120, bubbles: true, cancelable: true }));
      expect(left).toBe(120);

      // La rueda horizontal también sirve, y sumar no se queda a cero.
      await fireEvent(scroller, new WheelEvent('wheel', { deltaX: 60, bubbles: true, cancelable: true }));
      expect(left).toBe(180);
    });
  });

describe('tira de repos: separación y recorte de los nombres', () => {
  it('hay separación real entre pestañas (gap) y el nombre largo se recorta', () => {
    // Sin `gap` las pestañas se tocan y el nombre de una se mete en la vecina.
    const gap = /gap:\s*(\d+)px/.exec(regla('.scroller'));
    expect(gap, '`.scroller` tiene que declarar un gap en px').toBeTruthy();
    expect(Number(gap?.[1])).toBeGreaterThanOrEqual(6);

    // La pestaña recorta su contenido: nada se pinta fuera de su caja redondeada.
    expect(regla('.tab')).toContain('overflow: hidden');

    // El nombre es el que cede el sitio: se acorta con puntos suspensivos.
    const name = regla('.name');
    expect(name).toContain('flex: 1 1 auto');
    expect(name).toContain('min-width: 0');
    expect(name).toContain('text-overflow: ellipsis');

    // Y la pestaña no se encoge por debajo de su contenido útil.
    const min = /min-width:\s*(\d+)px/.exec(regla('.tab'));
    expect(Number(min?.[1])).toBeGreaterThanOrEqual(96);
  });
});

/**
 * El alias del repo se pone con clic derecho en la pestaña (menú contextual); el doble clic no hace nada.
 */
describe('TabStrip · alias del repositorio', () => {
  const props = (
    entries: Entry[],
    onalias: ReturnType<typeof vi.fn<(id: string) => void>>,
    activeId = 'a'
  ) => ({
    entries,
    activeId,
    onactivate: vi.fn(),
    onclose: vi.fn(),
    onnew: vi.fn(),
    onreorder: vi.fn(),
    onalias
  });

  it('el clic derecho abre el menú y pide el alias del repo de **esa** pestaña', async () => {
    const onalias = vi.fn<(id: string) => void>();
    render(TabStrip, { props: props([repo('a'), repo('b')], onalias) });
    await userEvent.pointer({ keys: '[MouseRight]', target: screen.getByRole('tab', { name: /^b/ }) });
    await userEvent.click(screen.getByTestId('tab-menu-alias'));
    expect(onalias).toHaveBeenCalledTimes(1);
    expect(onalias).toHaveBeenCalledWith('b');
  });

  it('el doble clic ya no pide alias, y la pestaña no enseña la rama', async () => {
    const onalias = vi.fn<(id: string) => void>();
    render(TabStrip, { props: props([repo('a')], onalias) });
    await userEvent.dblClick(screen.getByRole('tab', { name: /^a/ }));
    expect(onalias).not.toHaveBeenCalled();
    expect(document.querySelector('.branch')).toBeNull();
  });

  it('la pista del alias no contamina el nombre accesible', async () => {
    const onalias = vi.fn<(id: string) => void>();
    render(TabStrip, { props: props([repo('a')], onalias) });
    const tab = screen.getByRole('tab', { name: /^a/ });
    // El lector de pantalla no debe repetir «doble clic para poner alias» en cada pestaña: la pista
    // se enlaza con `aria-describedby`, que queda **fuera** del nombre accesible.
    const descrito = tab.getAttribute('aria-describedby');
    expect(descrito).toBeTruthy();
    expect(document.getElementById(descrito ?? '')?.textContent ?? '').toMatch(/alias/i);
    expect(tab.textContent ?? '').not.toMatch(/alias/i);
  });

  it('el clic derecho en la pestaña «nueva» no abre el menú de alias', async () => {
    const onalias = vi.fn<(id: string) => void>();
    render(TabStrip, { props: props([repo('a'), { id: 'new:1', kind: 'new' }], onalias, 'new:1') });
    await userEvent.pointer({ keys: '[MouseRight]', target: screen.getByRole('tab', { name: /nueva|new/i }) });
    expect(screen.queryByTestId('tab-menu')).toBeNull();
  });
});
