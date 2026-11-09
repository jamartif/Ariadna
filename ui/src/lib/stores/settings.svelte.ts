export type ThemeMode = 'system' | 'light' | 'dark';
export type Density = 'comfortable' | 'compact';

const KEY = 'ariadna.settings.v1';

export const DEFAULT_DIFF_FONT = 13;

function validFont(n: unknown): number {
  return typeof n === 'number' && Number.isInteger(n) && n >= 10 && n <= 24 ? n : DEFAULT_DIFF_FONT;
}

function load(): { theme: ThemeMode; density: Density; diffFontSize: number } {
  try {
    const raw = localStorage.getItem(KEY);
    if (raw) {
      const o = JSON.parse(raw) as Partial<{ theme: ThemeMode; density: Density; diffFontSize: number }>;
      return {
        theme: o.theme === 'light' || o.theme === 'dark' ? o.theme : 'system',
        density: o.density === 'compact' ? 'compact' : 'comfortable',
        diffFontSize: validFont(o.diffFontSize)
      };
    }
  } catch {
    /* almacenamiento no disponible */
  }
  return { theme: 'system', density: 'comfortable', diffFontSize: DEFAULT_DIFF_FONT };
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
  /** Tamaño del texto del diff (px). El valor de verdad está en `settings.json` (Rust). */
  diffFontSize = $state(DEFAULT_DIFF_FONT);
  /** Aviso a quien persiste en Rust cuando el usuario cambia tema o tamaño (barra de título, paleta). */
  onChange: ((s: { theme: ThemeMode; diffFontSize: number }) => void) | null = null;
  /** Vista de la lista de cambios: plana o en árbol por carpetas (se recuerda). */
  changesView = $state<ChangesView>(loadView());

  constructor() {
    const s = load();
    this.theme = s.theme;
    this.density = s.density;
    this.diffFontSize = s.diffFontSize;
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
    root.style.setProperty('--diff-font-size', `${this.diffFontSize}px`);
  }

  private save(): void {
    try {
      localStorage.setItem(KEY, JSON.stringify({ theme: this.theme, density: this.density, diffFontSize: this.diffFontSize }));
    } catch {
      /* ignorar */
    }
    this.apply();
  }

  setTheme(t: ThemeMode): void {
    this.theme = t;
    this.save();
    this.onChange?.({ theme: this.theme, diffFontSize: this.diffFontSize });
  }

  setDiffFontSize(n: number): void {
    this.diffFontSize = validFont(n);
    this.save();
    this.onChange?.({ theme: this.theme, diffFontSize: this.diffFontSize });
  }

  /** Aplica valores que vienen de Rust (o una vista previa) sin avisar de vuelta. */
  applyExternal(theme: ThemeMode, diffFontSize: number): void {
    this.theme = theme;
    this.diffFontSize = validFont(diffFontSize);
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
    return Math.round(this.diffFontSize * (this.density === 'compact' ? 1.385 : 1.615));
  }
}

export const settings = new Settings();
