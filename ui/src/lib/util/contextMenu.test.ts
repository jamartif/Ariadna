import { afterEach, describe, expect, it } from 'vitest';
import { installContextMenuGuard } from './contextMenu';

let off: (() => void) | null = null;
afterEach(() => {
  off?.();
  off = null;
  document.body.innerHTML = '';
});

function rightClick(el: Element): boolean {
  const ev = new MouseEvent('contextmenu', { bubbles: true, cancelable: true });
  el.dispatchEvent(ev);
  return ev.defaultPrevented;
}

describe('menú contextual', () => {
  it('se suprime fuera de campos de texto', () => {
    off = installContextMenuGuard();
    document.body.innerHTML = '<div id="a">hola</div>';
    expect(rightClick(document.getElementById('a')!)).toBe(true);
  });

  it('se conserva en input y textarea', () => {
    off = installContextMenuGuard();
    document.body.innerHTML = '<input id="i"><textarea id="t"></textarea>';
    expect(rightClick(document.getElementById('i')!)).toBe(false);
    expect(rightClick(document.getElementById('t')!)).toBe(false);
  });

  it('deja de actuar al desinstalar', () => {
    off = installContextMenuGuard();
    off();
    off = null;
    document.body.innerHTML = '<div id="a">x</div>';
    expect(rightClick(document.getElementById('a')!)).toBe(false);
  });
});
