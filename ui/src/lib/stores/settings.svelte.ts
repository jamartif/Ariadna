export type ThemeMode = 'system' | 'light' | 'dark';
export type Density = 'comfortable' | 'compact';

const KEY = 'ariadna.settings.v1';

function load(): { theme: ThemeMode; density: Density } {
  try {
    const raw = localStorage.getItem(KEY);
    if (raw) {
      const o = JSON.parse(raw) as Partial<{ theme: ThemeMode; density: Density }>;
      return {
        theme: o.theme === 'light' || o.theme === 'dark' ? o.theme : 'system',
        density: o.density === 'compact' ? 'compact' : 'comfortable'
      };
    }
  } catch {
    /* almacenamiento no disponible */
  }
  return { theme: 'system', density: 'comfortable' };
}

export type ChangesView = 'list' | 'tree';
const VIEW_KEY = 'ariadna.changesView.v1';

function loadView(): ChangesView {
  try {
    return localStorage.getItem(VIEW_KEY) === 'tree' ? 'tree' : 'list';
  } catch {
    return 'list';
  }
}

class Settings {
  theme = $state<ThemeMode>('system');
  density = $state<Density>('comfortable');
  /** Vista de la lista de cambios: plana o en árbol por carpetas (se recuerda). */
  changesView = $state<ChangesView>(loadView());

  constructor() {
    const s = load();
    this.theme = s.theme;
    this.density = s.density;
  }

  setChangesView(v: ChangesView): void {
    this.changesView = v;
    try {
      localStorage.setItem(VIEW_KEY, v);
    } catch {
      /* ignorar */
    }
  }

  apply(root: HTMLElement = document.documentElement): void {
    if (this.theme === 'system') root.removeAttribute('data-theme');
    else root.setAttribute('data-theme', this.theme);
    root.setAttribute('data-density', this.density);
  }

  private save(): void {
    try {
      localStorage.setItem(KEY, JSON.stringify({ theme: this.theme, density: this.density }));
    } catch {
      /* ignorar */
    }
    this.apply();
  }

  setTheme(t: ThemeMode): void {
    this.theme = t;
    this.save();
  }

  setDensity(d: Density): void {
    this.density = d;
    this.save();
  }

  cycleTheme(): void {
    this.setTheme(this.theme === 'system' ? 'light' : this.theme === 'light' ? 'dark' : 'system');
  }

  toggleDensity(): void {
    this.setDensity(this.density === 'comfortable' ? 'compact' : 'comfortable');
  }

  /** Alturas de fila (px) para listas virtualizadas. Deben coincidir con `--row-h` de tokens.css. */
  get rowHeight(): number {
    return this.density === 'compact' ? 24 : 28;
  }
  get headerHeight(): number {
    return this.density === 'compact' ? 28 : 32;
  }
  get codeLineHeight(): number {
    return this.density === 'compact' ? 18 : 21;
  }
}

export const settings = new Settings();
