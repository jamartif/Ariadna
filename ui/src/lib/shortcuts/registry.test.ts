import { afterEach, describe, expect, it, vi } from 'vitest';
import { es } from '$lib/i18n/es';
import {
  SHORTCUTS,
  dispatchKey,
  findConflicts,
  formatCombo,
  matches,
  normalizeCombo,
  registerHandler,
  type ShortcutDef
} from './registry';

function key(init: KeyboardEventInit & { key: string }, target?: Element): KeyboardEvent {
  const e = new KeyboardEvent('keydown', { bubbles: true, cancelable: true, ...init });
  if (target) Object.defineProperty(e, 'target', { value: target });
  return e;
}

describe('registro único de atajos', () => {
  it('no tiene conflictos (en Windows/Linux ni en macOS)', () => {
    expect(findConflicts(SHORTCUTS).map(([a, b]) => `${a.id} ↔ ${b.id}`)).toEqual([]);
  });

  it('los ids son únicos y todos tienen texto en el catálogo', () => {
    const ids = SHORTCUTS.map((s) => s.id);
    expect(new Set(ids).size).toBe(ids.length);
    for (const s of SHORTCUTS) expect(es, s.label).toHaveProperty([s.label]);
  });

  it('cubre los atajos de la spec §8 y §7', () => {
    const ids = SHORTCUTS.map((s) => s.id);
    for (const id of ['tab.new', 'tab.close', 'tab.next', 'tab.prev', 'tab.moveLeft', 'tab.moveRight', 'palette.open', 'commit.submit']) {
      expect(ids).toContain(id);
    }
    for (let i = 1; i <= 9; i++) expect(ids).toContain(`tab.goto.${i}`);
    expect(SHORTCUTS.find((s) => s.id === 'commit.submit')?.scope).toBe('editor');
  });

  it('el detector encuentra conflictos reales', () => {
    const a: ShortcutDef = { id: 'a', combo: 'Mod+T', scope: 'global', label: 'shortcut.newTab' };
    const b: ShortcutDef = { id: 'b', combo: 'Ctrl+T', scope: 'tab', label: 'shortcut.newTab' };
    const c: ShortcutDef = { id: 'c', combo: 'Mod+J', scope: 'tab', label: 'shortcut.newTab' };
    const d: ShortcutDef = { id: 'd', combo: 'Mod+J', scope: 'editor', label: 'shortcut.newTab' };
    const e: ShortcutDef = { id: 'e', combo: 'Mod+J', scope: 'tab', label: 'shortcut.newTab' };
    // global ↔ tab con la misma tecla (Mod = Ctrl en Windows): conflicto.
    expect(findConflicts([a, b])).toHaveLength(1);
    // tab ↔ editor distintos y sin global: no chocan.
    expect(findConflicts([c, d])).toHaveLength(0);
    // mismo ámbito: chocan.
    expect(findConflicts([c, e])).toHaveLength(1);
  });

  it('Mod es ⌘ en macOS y Ctrl en Windows/Linux', () => {
    expect(normalizeCombo('Mod+T', true)).toBe('meta+t');
    expect(normalizeCombo('Mod+T', false)).toBe('ctrl+t');
    expect(normalizeCombo('Ctrl+Tab', true)).toBe('ctrl+tab');
    const newTab = SHORTCUTS.find((s) => s.id === 'tab.new') as ShortcutDef;
    expect(matches(key({ key: 't', ctrlKey: true }), newTab, false)).toBe(true);
    expect(matches(key({ key: 't', ctrlKey: true }), newTab, true)).toBe(false);
    expect(matches(key({ key: 't', metaKey: true }), newTab, true)).toBe(true);
    expect(formatCombo('Mod+K', true)).toBe('⌘K');
    expect(formatCombo('Mod+K', false)).toBe('Ctrl+K');
    expect(formatCombo('Ctrl+Shift+Tab', false)).toBe('Ctrl+Mayús+Tab');
  });
});

describe('despacho', () => {
  const offs: (() => void)[] = [];
  afterEach(() => {
    offs.splice(0).forEach((f) => f());
    document.body.innerHTML = '';
  });

  it('ejecuta el manejador registrado y evita el comportamiento por defecto', () => {
    const fn = vi.fn();
    offs.push(registerHandler('tab.new', fn));
    const e = key({ key: 't', ctrlKey: true });
    expect(dispatchKey(e, false)).toBe(true);
    expect(fn).toHaveBeenCalledOnce();
    expect(e.defaultPrevented).toBe(true);
  });

  it('Ctrl+1…9 y Ctrl+Tab / Ctrl+Mayús+Tab', () => {
    const goto3 = vi.fn();
    const next = vi.fn();
    const prev = vi.fn();
    offs.push(registerHandler('tab.goto.3', goto3), registerHandler('tab.next', next), registerHandler('tab.prev', prev));
    dispatchKey(key({ key: '3', ctrlKey: true }), false);
    dispatchKey(key({ key: 'Tab', ctrlKey: true }), false);
    dispatchKey(key({ key: 'Tab', ctrlKey: true, shiftKey: true }), false);
    expect([goto3, next, prev].map((f) => f.mock.calls.length)).toEqual([1, 1, 1]);
  });

  it('el atajo de ámbito editor solo actúa dentro de un editor', () => {
    const fn = vi.fn();
    offs.push(registerHandler('commit.submit', fn));
    const outside = document.createElement('div');
    const editor = document.createElement('div');
    editor.setAttribute('data-scope', 'editor');
    const ta = document.createElement('textarea');
    editor.append(ta);
    document.body.append(outside, editor);
    expect(dispatchKey(key({ key: 'Enter', ctrlKey: true }, outside), false)).toBe(false);
    expect(fn).not.toHaveBeenCalled();
    expect(dispatchKey(key({ key: 'Enter', ctrlKey: true }, ta), false)).toBe(true);
    expect(fn).toHaveBeenCalledOnce();
  });

  it('un manejador que devuelve false declina el evento', () => {
    offs.push(registerHandler('tab.close', () => false));
    const e = key({ key: 'w', ctrlKey: true });
    expect(dispatchKey(e, false)).toBe(false);
    expect(e.defaultPrevented).toBe(false);
  });
});
