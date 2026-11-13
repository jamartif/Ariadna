/**
 * REGISTRO ÚNICO de atajos (spec §7). Todo atajo de la aplicación se declara
 * aquí; ningún componente escucha `keydown` por su cuenta. `Mod` es ⌘ en macOS
 * y Ctrl en Windows/Linux. `findConflicts` lo comprueba un test.
 *
 * Ámbitos:
 *  - `global`: en cualquier parte de la ventana.
 *  - `tab`: con una pestaña de repositorio activa.
 *  - `editor`: solo cuando el foco está dentro de un elemento `[data-scope~="editor"]`.
 */
export type ShortcutScope = 'global' | 'tab' | 'editor';

export interface ShortcutDef {
  id: string;
  combo: string;
  scope: ShortcutScope;
  /** Clave del catálogo i18n. */
  label: string;
}

const gotoTab: ShortcutDef[] = Array.from({ length: 9 }, (_, i) => ({
  id: `tab.goto.${i + 1}`,
  combo: `Mod+${i + 1}`,
  scope: 'global' as const,
  label: `shortcut.gotoTab${i + 1}`
}));

export const SHORTCUTS: readonly ShortcutDef[] = [
  { id: 'tab.new', combo: 'Mod+T', scope: 'global', label: 'shortcut.newTab' },
  { id: 'tab.close', combo: 'Mod+W', scope: 'global', label: 'shortcut.closeTab' },
  ...gotoTab,
  { id: 'tab.next', combo: 'Ctrl+Tab', scope: 'global', label: 'shortcut.nextTab' },
  { id: 'tab.prev', combo: 'Ctrl+Shift+Tab', scope: 'global', label: 'shortcut.prevTab' },
  { id: 'tab.moveLeft', combo: 'Alt+Shift+ArrowLeft', scope: 'global', label: 'shortcut.moveTabLeft' },
  { id: 'tab.moveRight', combo: 'Alt+Shift+ArrowRight', scope: 'global', label: 'shortcut.moveTabRight' },
  { id: 'repo.open', combo: 'Mod+O', scope: 'global', label: 'shortcut.openRepo' },
  { id: 'palette.open', combo: 'Mod+K', scope: 'global', label: 'shortcut.palette' },
  { id: 'changes.refresh', combo: 'Mod+R', scope: 'tab', label: 'shortcut.refresh' },
  { id: 'changes.stageAll', combo: 'Mod+Shift+A', scope: 'tab', label: 'shortcut.stageAll' },
  { id: 'changes.unstageAll', combo: 'Mod+Shift+U', scope: 'tab', label: 'shortcut.unstageAll' },
  { id: 'commit.focus', combo: 'Mod+L', scope: 'tab', label: 'shortcut.focusCommit' },
  // Mod+Shift+P (no Mod+P: lo usa imprimir en la WebView).
  { id: 'pr.open', combo: 'Mod+Shift+P', scope: 'tab', label: 'shortcut.pullRequests' },
  // Líneas sueltas y stash.
  { id: 'lines.apply', combo: 'Mod+Shift+Enter', scope: 'tab', label: 'shortcut.applyLines' },
  { id: 'stash.push', combo: 'Mod+Shift+S', scope: 'tab', label: 'shortcut.stashPush' },
  { id: 'stash.open', combo: 'Mod+Shift+H', scope: 'tab', label: 'shortcut.stashOpen' },
  { id: 'commit.submit', combo: 'Mod+Enter', scope: 'editor', label: 'shortcut.commit' },
  // Mod+Shift+G (no Alt: en macOS ⌥G produce «©» y la tecla no coincide).
  { id: 'ai.generate', combo: 'Mod+Shift+G', scope: 'tab', label: 'shortcut.aiGenerate' },
  // Estudio de conflictos: solo con el foco dentro del editor (`data-scope="editor"`).
  { id: 'conflicts.next', combo: 'Alt+ArrowDown', scope: 'editor', label: 'shortcut.conflictNext' },
  { id: 'conflicts.prev', combo: 'Alt+ArrowUp', scope: 'editor', label: 'shortcut.conflictPrev' },
  { id: 'conflicts.ours', combo: 'Alt+1', scope: 'editor', label: 'shortcut.conflictOurs' },
  { id: 'conflicts.theirs', combo: 'Alt+2', scope: 'editor', label: 'shortcut.conflictTheirs' },
  { id: 'conflicts.both', combo: 'Alt+3', scope: 'editor', label: 'shortcut.conflictBoth' },
  { id: 'clone.open', combo: 'Mod+Shift+O', scope: 'global', label: 'shortcut.clone' },
  { id: 'settings.open', combo: 'Mod+,', scope: 'global', label: 'shortcut.settings' }
];

interface Parsed {
  mod: boolean;
  ctrl: boolean;
  alt: boolean;
  shift: boolean;
  meta: boolean;
  key: string;
}

export function parseCombo(combo: string): Parsed {
  const parts = combo.split('+');
  const key = (parts.pop() ?? '').toLowerCase();
  const set = new Set(parts.map((p) => p.toLowerCase()));
  return {
    mod: set.has('mod'),
    ctrl: set.has('ctrl'),
    alt: set.has('alt'),
    shift: set.has('shift'),
    meta: set.has('meta'),
    key
  };
}

/** Forma canónica dependiente de plataforma: modificadores ordenados + tecla. */
export function normalizeCombo(combo: string, mac: boolean): string {
  const p = parseCombo(combo);
  const ctrl = p.ctrl || (p.mod && !mac);
  const meta = p.meta || (p.mod && mac);
  return [ctrl ? 'ctrl' : '', meta ? 'meta' : '', p.alt ? 'alt' : '', p.shift ? 'shift' : '', p.key]
    .filter(Boolean)
    .join('+');
}

export function eventCombo(e: Pick<KeyboardEvent, 'ctrlKey' | 'metaKey' | 'altKey' | 'shiftKey' | 'key'>): string {
  return [e.ctrlKey ? 'ctrl' : '', e.metaKey ? 'meta' : '', e.altKey ? 'alt' : '', e.shiftKey ? 'shift' : '', e.key.toLowerCase()]
    .filter(Boolean)
    .join('+');
}

export function matches(e: KeyboardEvent, def: ShortcutDef, mac: boolean): boolean {
  return eventCombo(e) === normalizeCombo(def.combo, mac);
}

const SYMBOLS: Record<string, string> = {
  arrowleft: '←',
  arrowright: '→',
  arrowup: '↑',
  arrowdown: '↓',
  enter: 'Intro',
  tab: 'Tab'
};

/** Texto para mostrar, p. ej. `Ctrl+Mayús+Tab` o `⌘K`. */
export function formatCombo(combo: string, mac: boolean): string {
  const p = parseCombo(combo);
  const key = SYMBOLS[p.key] ?? p.key.toUpperCase();
  if (mac) {
    return `${p.ctrl ? '⌃' : ''}${p.alt ? '⌥' : ''}${p.shift ? '⇧' : ''}${p.meta || p.mod ? '⌘' : ''}${key}`;
  }
  return [p.ctrl || p.mod ? 'Ctrl' : '', p.alt ? 'Alt' : '', p.shift ? 'Mayús' : '', key].filter(Boolean).join('+');
}

/**
 * Dos atajos chocan si tienen la misma combinación y comparten ámbito, o uno es
 * global (que siempre está activo). Se evalúa para ambas plataformas.
 */
export function findConflicts(defs: readonly ShortcutDef[] = SHORTCUTS): [ShortcutDef, ShortcutDef][] {
  const out: [ShortcutDef, ShortcutDef][] = [];
  for (const mac of [false, true]) {
    for (let i = 0; i < defs.length; i++) {
      for (let j = i + 1; j < defs.length; j++) {
        const a = defs[i] as ShortcutDef;
        const b = defs[j] as ShortcutDef;
        if (normalizeCombo(a.combo, mac) !== normalizeCombo(b.combo, mac)) continue;
        if (a.scope === b.scope || a.scope === 'global' || b.scope === 'global') {
          if (!out.some(([x, y]) => x === a && y === b)) out.push([a, b]);
        }
      }
    }
  }
  return out;
}

export function isMac(): boolean {
  if (typeof navigator === 'undefined') return false;
  const nav = navigator as Navigator & { userAgentData?: { platform?: string } };
  const platform = nav.userAgentData?.platform ?? nav.platform ?? '';
  return /mac|iphone|ipad/i.test(platform);
}

// ---- Despacho -------------------------------------------------------------------

/** Un manejador devuelve `false` para declinar (el evento sigue su curso). */
export type ShortcutHandler = () => boolean | void;

const handlers = new Map<string, ShortcutHandler[]>();

export function registerHandler(id: string, fn: ShortcutHandler): () => void {
  const list = handlers.get(id) ?? [];
  list.push(fn);
  handlers.set(id, list);
  return () => {
    const cur = handlers.get(id) ?? [];
    const i = cur.lastIndexOf(fn);
    if (i >= 0) cur.splice(i, 1);
  };
}

export function registerHandlers(map: Record<string, ShortcutHandler>): () => void {
  const offs = Object.entries(map).map(([id, fn]) => registerHandler(id, fn));
  return () => offs.forEach((off) => off());
}

/** Ejecuta el atajo por id (lo usa también la paleta de comandos). */
export function runShortcut(id: string): boolean {
  const list = handlers.get(id);
  const fn = list?.[list.length - 1];
  if (!fn) return false;
  return fn() !== false;
}

/** Última ejecución de cada atajo por teclado (para no repetir la que ya hizo el menú nativo). */
const keyRuns = new Map<string, number>();
/** Ventana en la que un elemento de menú se considera eco de la tecla que acaba de ejecutarse. */
export const MENU_ECHO_MS = 250;

/**
 * Ejecuta el atajo `id` por un elemento del menú nativo (evento `menu-action`). Si la misma tecla
 * lo acaba de ejecutar (aceleradores a la vez en menú y en la ventana) se ignora el eco, de modo
 * que nunca se ejecuta dos veces. Devuelve `true` si había manejador o era un eco.
 */
export function runMenuAction(id: string, now = Date.now()): boolean {
  const last = keyRuns.get(id);
  if (last !== undefined && now - last >= 0 && now - last < MENU_ECHO_MS) return true;
  return runShortcut(id);
}

export function dispatchKey(e: KeyboardEvent, mac = isMac()): boolean {
  for (const def of SHORTCUTS) {
    if (!matches(e, def, mac)) continue;
    if (def.scope === 'editor') {
      const t = e.target as Element | null;
      if (!t || typeof t.closest !== 'function' || !t.closest('[data-scope~="editor"]')) continue;
    }
    const list = handlers.get(def.id);
    const fn = list?.[list.length - 1];
    if (!fn) continue;
    if (fn() === false) continue;
    keyRuns.set(def.id, Date.now());
    e.preventDefault();
    e.stopPropagation();
    return true;
  }
  return false;
}

/** Instala el único listener de teclado de la aplicación. */
export function installShortcuts(target: Window | Document = window, mac = isMac()): () => void {
  const onKey = (e: Event) => {
    dispatchKey(e as KeyboardEvent, mac);
  };
  target.addEventListener('keydown', onKey, true);
  return () => target.removeEventListener('keydown', onKey, true);
}
