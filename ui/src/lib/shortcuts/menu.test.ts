import { describe, expect, it } from 'vitest';
import menu from '$lib/api/fixtures/menu.json';
import { SHORTCUTS, parseCombo, type ShortcutDef } from './registry';

// `menu.json` lo genera el test `core::menu` de Rust (el menú nativo). Aquí se comprueba que cada
// elemento con atajo usa el mismo combo que el registro único de atajos.

interface Item {
  kind: 'item';
  id: string;
  label: string;
  accelerator: string | null;
  effect: { type: 'shortcut' | 'quit' | 'url'; value?: string };
}
type Entry = { kind: 'separator' } | { kind: 'role'; role: string } | Item;
interface Submenu {
  label: string;
  entries: Entry[];
}

const KEYS: Record<string, string> = { arrowleft: 'Left', arrowright: 'Right', arrowup: 'Up', arrowdown: 'Down', enter: 'Enter', tab: 'Tab' };

/** Combo del registro → acelerador de Tauri (`Mod` = `CmdOrCtrl`). */
function accelerator(def: ShortcutDef): string {
  const p = parseCombo(def.combo);
  const key = KEYS[p.key] ?? (p.key.length === 1 ? p.key.toUpperCase() : p.key);
  return [p.mod ? 'CmdOrCtrl' : '', p.ctrl ? 'Ctrl' : '', p.alt ? 'Alt' : '', p.shift ? 'Shift' : '', key].filter(Boolean).join('+');
}

const variants = menu as unknown as { mac: Submenu[]; other: Submenu[] };
const items = (m: Submenu[]): Item[] => m.flatMap((s) => s.entries).filter((e): e is Item => e.kind === 'item');

describe('menú nativo ↔ registro único de atajos', () => {
  for (const [name, m] of Object.entries(variants)) {
    it(`(${name}) cada elemento con atajo usa el combo del registro`, () => {
      const shortcuts = items(m).filter((i) => i.effect.type === 'shortcut');
      expect(shortcuts.length).toBeGreaterThan(5);
      for (const item of shortcuts) {
        const def = SHORTCUTS.find((s) => s.id === item.effect.value);
        expect(def, `el atajo «${item.effect.value}» no existe en el registro`).toBeDefined();
        expect(item.id).toBe(def!.id);
        expect(item.accelerator, item.id).toBe(accelerator(def!));
        // Un atajo de ámbito «editor» no puede salir de un menú.
        expect(def!.scope, item.id).not.toBe('editor');
      }
    });

    it(`(${name}) tiene Archivo, Editar, Ver, Ventana y Ayuda`, () => {
      const labels = m.map((s) => s.label);
      for (const l of ['Archivo', 'Editar', 'Ver', 'Ventana', 'Ayuda']) expect(labels).toContain(l);
    });
  }

  it('⌘O / Ctrl+O abre repositorio y ⌘R / Ctrl+R refresca', () => {
    for (const m of Object.values(variants)) {
      const byId = new Map(items(m).map((i) => [i.id, i]));
      expect(byId.get('repo.open')?.accelerator).toBe('CmdOrCtrl+O');
      expect(byId.get('changes.refresh')?.accelerator).toBe('CmdOrCtrl+R');
    }
  });

  it('mover pestañas queda fuera del menú (en macOS le quitaría a los campos de texto «seleccionar por palabras»)', () => {
    for (const m of Object.values(variants)) {
      const ids = items(m).map((i) => i.id);
      expect(ids).not.toContain('tab.moveLeft');
      expect(ids).not.toContain('tab.moveRight');
    }
  });

  it('solo macOS lleva el menú de aplicación y el acelerador de salir', () => {
    expect(variants.mac[0]!.label).toBe('Ariadna');
    expect(variants.other[0]!.label).toBe('Archivo');
    expect(items(variants.mac).find((i) => i.id === 'app.quit')?.accelerator).toBe('CmdOrCtrl+Q');
    expect(items(variants.other).find((i) => i.id === 'app.quit')?.accelerator).toBeNull();
  });
});
