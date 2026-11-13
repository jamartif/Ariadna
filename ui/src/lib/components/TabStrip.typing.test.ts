import { render, screen } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { installShortcuts } from '$lib/shortcuts/registry';
import type { Entry } from '$lib/stores/app.svelte';
import TabStrip from './TabStrip.svelte';

function repo(id: string): Entry {
  return { id, kind: 'repo', info: { tabId: id, path: `/r/${id}`, name: id, branch: 'main', detached: false, changeCount: 0, conflictCount: 0, busy: null, missing: false } };
}

let off: () => void;
beforeEach(() => {
  off = installShortcuts(window, false);
});
afterEach(() => {
  off();
  document.body.querySelectorAll('[data-test-field]').forEach((e) => e.remove());
});

describe('reordenar pestañas con el teclado sin pisar el texto', () => {
  it('Alt+Mayús+→ con el foco en un campo de texto no reordena (es «seleccionar por palabras»)', async () => {
    const onreorder = vi.fn();
    render(TabStrip, { props: { entries: [repo('a'), repo('b')], activeId: 'a', onactivate: vi.fn(), onclose: vi.fn(), onnew: vi.fn(), onreorder } });
    const field = document.createElement('textarea');
    field.setAttribute('data-test-field', '');
    document.body.appendChild(field);
    field.focus();
    const ev = new KeyboardEvent('keydown', { key: 'ArrowRight', altKey: true, shiftKey: true, bubbles: true, cancelable: true });
    field.dispatchEvent(ev);
    expect(onreorder).not.toHaveBeenCalled();
    expect(ev.defaultPrevented).toBe(false);
  });

  it('con el foco en la barra de pestañas sí reordena', async () => {
    const onreorder = vi.fn();
    render(TabStrip, { props: { entries: [repo('a'), repo('b')], activeId: 'a', onactivate: vi.fn(), onclose: vi.fn(), onnew: vi.fn(), onreorder } });
    (screen.getAllByRole('tab')[0] as HTMLElement).focus();
    await userEvent.keyboard('{Alt>}{Shift>}{ArrowRight}{/Shift}{/Alt}');
    expect(onreorder).toHaveBeenCalledWith(['b', 'a']);
  });
});
