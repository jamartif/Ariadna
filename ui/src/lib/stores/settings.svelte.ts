import { applyLanguage, resolveLocale, type LanguagePref } from '$lib/i18n';

export type { LanguagePref };
export type ThemeMode = 'system' | 'light' | 'dark';
export type Density = 'comfortable' | 'compact';

const KEY = 'ariadna.settings.v1';

export const DEFAULT_DIFF_FONT = 13;

function validFont(n: unknown): number {
  return typeof n === 'number' && Number.isInteger(n) && n >= 10 && n <= 24 ? n : DEFAULT_DIFF_FONT;
}

function validLanguage(v: unknown): LanguagePref {
  return v === 'es' || v === 'en' || v === 'system' ? v : 'system';
}

function load(): { theme: ThemeMode; density: Density; diffFontSize: number; language: LanguagePref } {
  try {
    const raw = localStorage.getItem(KEY);
    if (raw) {
      const o = JSON.parse(raw) as Partial<{ theme: ThemeMode; density: Density; diffFontSize: number; language: LanguagePref }>;
      return {
        theme: o.theme === 'light' || o.theme === 'dark' ? o.theme : 'system',
        density: o.density === 'compact' ? 'compact' : 'comfortable',
        diffFontSize: validFont(o.diffFontSize),
        language: validLanguage(o.language)
      };
    }
  } catch {
    /* almacenamiento no disponible */
  }
  return { theme: 'system', density: 'comfortable', diffFontSize: DEFAULT_DIFF_FONT, language: 'system' };
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
  /** Idioma preferido (`system` = el del sistema operativo). Espejo de `settings.json`. */
  language = $state<LanguagePref>('system');
  /** Aviso a quien persiste en Rust cuando el usuario cambia tema, tamaño o idioma. */
  onChange: ((s: { theme: ThemeMode; diffFontSize: number; language: LanguagePref }) => void) | null = null;
  /** Vista de la lista de cambios: plana o en árbol por carpetas (se recuerda). */
  changesView = $state<ChangesView>(loadView());

  constructor() {
    const s = load();
    this.theme = s.theme;
    this.density = s.density;
    this.diffFontSize = s.diffFontSize;
    this.language = s.language;
    // El idioma del espejo local se aplica ya: el primer render sale en el idioma correcto.
    applyLanguage(this.language);
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
    root.setAttribute('lang', resolveLocale(this.language));
  }

  private save(): void {
    try {
      localStorage.setItem(
        KEY,
        JSON.stringify({ theme: this.theme, density: this.density, diffFontSize: this.diffFontSize, language: this.language })
      );
    } catch {
      /* ignorar */
    }
    this.apply();
  }

  setTheme(t: ThemeMode): void {
    this.theme = t;
    this.save();
    this.onChange?.({ theme: this.theme, diffFontSize: this.diffFontSize, language: this.language });
  }

  setDiffFontSize(n: number): void {
    this.diffFontSize = validFont(n);
    this.save();
    this.onChange?.({ theme: this.theme, diffFontSize: this.diffFontSize, language: this.language });
  }

  /**
   * Cambia el idioma de la interfaz y lo aplica al momento (sin reiniciar): catálogo de
   * `t()`, `<html lang>` y aviso a quien persiste en Rust.
   */
  setLanguage(pref: LanguagePref): void {
    this.language = validLanguage(pref);
    applyLanguage(this.language);
    this.save();
    this.onChange?.({ theme: this.theme, diffFontSize: this.diffFontSize, language: this.language });
  }

  /** Aplica valores que vienen de Rust (o una vista previa) sin avisar de vuelta. */
  applyExternal(theme: ThemeMode, diffFontSize: number, language: LanguagePref = this.language): void {
    this.theme = theme;
    this.diffFontSize = validFont(diffFontSize);
    this.language = validLanguage(language);
    applyLanguage(this.language);
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
