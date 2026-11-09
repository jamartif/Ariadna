import type { Api } from '$lib/api/api';
import {
  DIFF_FONT_MAX,
  DIFF_FONT_MIN,
  toApiError,
  type AiProfileKind,
  type AiSettingsPatch,
  type AiState,
  type ApiError,
  type AppSettings,
  type CliStatus,
  type GitDetect,
  type GitIdentity,
  type ProfileRule,
  type SettingsView,
  type ThemePref
} from '$lib/api/types';
import { t } from '$lib/i18n';
import { normalizePattern } from '$lib/util/profileRules';
import { settings as globalSettings } from './settings.svelte';
import { toasts } from './toasts.svelte';

export type SettingsSection = 'general' | 'git' | 'ai' | 'accounts' | 'about';
export const SETTINGS_SECTIONS: readonly SettingsSection[] = ['general', 'git', 'ai', 'accounts', 'about'];

/** Estado del diálogo de Ajustes (si está abierto y en qué sección). Uno solo por ventana. */
class SettingsUi {
  isOpen = $state(false);
  section = $state<SettingsSection>('general');

  open(section: SettingsSection = this.section): void {
    this.section = section;
    this.isOpen = true;
  }
  close(): void {
    this.isOpen = false;
  }
  toggle(): void {
    if (this.isOpen) this.close();
    else this.open();
  }
}

export const settingsUi = new SettingsUi();

export type RuleProblem = 'empty' | 'long' | 'scheme' | 'chars' | 'host' | 'segment' | 'duplicate';

const clone = <T>(v: T): T => JSON.parse(JSON.stringify(v)) as T;

/**
 * Lógica de la pantalla de Ajustes. Edita un BORRADOR de `settings.json` (General, Git y reglas)
 * que se guarda de una vez; los ajustes de IA (activar, perfil, proveedor) se aplican al
 * momento porque viven en `ai-settings.json` y tienen su propio comando.
 */
export class SettingsPanel {
  view = $state.raw<SettingsView | null>(null);
  draft = $state<AppSettings | null>(null);
  loadError = $state.raw<ApiError | null>(null);
  saveError = $state.raw<ApiError | null>(null);
  saving = $state(false);

  identity = $state.raw<GitIdentity | null>(null);
  /** Resultado de la última comprobación de la ruta de git. */
  gitCheck = $state.raw<GitDetect | null>(null);
  gitChecking = $state(false);

  /** Ajustes de IA tal como se guardaron (perfil elegido en Ajustes, sin aplicar reglas). */
  ai = $state.raw<AiState | null>(null);
  /** Estado de IA de la pestaña actual con su regla repo → perfil aplicada (solo si hay pestaña). */
  effective = $state.raw<AiState | null>(null);
  cli = $state.raw<CliStatus[] | null>(null);
  cliLoading = $state(false);
  aiBusy = $state(false);

  private original: { theme: ThemePref; diffFontSize: number } | null = null;
  private disposed = false;
  private checkRun = 0;

  constructor(
    readonly api: Api,
    /** Pestaña activa (si hay repo abierto): sirve para mostrar su regla repo → perfil. */
    readonly tabId: string | undefined = undefined
  ) {}

  // ---- Carga ---------------------------------------------------------------------

  async load(): Promise<void> {
    try {
      const view = await this.api.getSettings();
      if (this.disposed) return;
      this.view = view;
      this.draft = clone(view.settings);
      this.original = { theme: view.settings.theme, diffFontSize: view.settings.diffFontSize };
      this.loadError = null;
    } catch (e) {
      this.loadError = toApiError(e);
    }
    void this.loadIdentity();
    void this.loadAi();
    void this.loadCli();
  }

  private async loadIdentity(): Promise<void> {
    try {
      const id = await this.api.gitIdentity();
      if (!this.disposed) this.identity = id;
    } catch {
      /* solo lectura y opcional */
    }
  }

  /** `aiUpdateSettings` con un parche vacío es la lectura que no necesita pestaña. */
  async loadAi(): Promise<void> {
    try {
      const s = await this.api.aiUpdateSettings({});
      if (!this.disposed) this.ai = s;
    } catch {
      /* la IA es opcional */
    }
    await this.loadEffective();
  }

  private async loadEffective(): Promise<void> {
    if (!this.tabId) return;
    try {
      const s = await this.api.aiUpdateSettings({ tabId: this.tabId });
      if (!this.disposed) this.effective = s;
    } catch {
      /* sin regla que mostrar */
    }
  }

  async loadCli(): Promise<void> {
    this.cliLoading = true;
    try {
      const c = await this.api.getCliStatus();
      if (!this.disposed) this.cli = c;
    } catch {
      /* se queda sin estado: la sección lo muestra */
    } finally {
      this.cliLoading = false;
    }
  }

  dispose(): void {
    this.disposed = true;
    this.checkRun++;
  }

  // ---- Borrador ---------------------------------------------------------------------

  get dirty(): boolean {
    return !!this.view && !!this.draft && JSON.stringify(this.view.settings) !== JSON.stringify(this.draft);
  }

  get fontProblem(): boolean {
    const n = this.draft?.diffFontSize;
    return !Number.isInteger(n) || (n ?? 0) < DIFF_FONT_MIN || (n ?? 0) > DIFF_FONT_MAX;
  }

  /** El tema se ve al instante; «Cancelar» lo devuelve a lo guardado. */
  setTheme(theme: ThemePref): void {
    if (!this.draft) return;
    this.draft.theme = theme;
    globalSettings.applyExternal(theme, globalSettings.diffFontSize);
  }

  setDiffFontSize(n: number): void {
    if (!this.draft) return;
    this.draft.diffFontSize = n;
    if (Number.isInteger(n) && n >= DIFF_FONT_MIN && n <= DIFF_FONT_MAX) {
      globalSettings.applyExternal(globalSettings.theme, n);
    }
  }

  setGitPath(path: string): void {
    if (!this.draft) return;
    this.draft.gitPath = path.trim() === '' ? null : path;
    this.gitCheck = null;
  }

  /** Vuelve a lo guardado (también la vista previa de tema y tamaño). */
  revert(): void {
    if (this.view) this.draft = clone(this.view.settings);
    if (this.original) globalSettings.applyExternal(this.original.theme, this.original.diffFontSize);
    this.saveError = null;
    this.gitCheck = null;
  }

  // ---- Git -----------------------------------------------------------------------------

  /** «Detectar»: busca git sin tocar lo escrito en el campo y, si lo encuentra, lo rellena. */
  async detectGit(): Promise<void> {
    await this.runCheck(null, true);
  }

  /** Comprueba la ruta escrita (o la detección automática si está vacía). */
  async checkGit(): Promise<void> {
    await this.runCheck(this.draft?.gitPath ?? null, false);
  }

  private async runCheck(path: string | null, fill: boolean): Promise<void> {
    const mine = ++this.checkRun;
    this.gitChecking = true;
    try {
      const r = await this.api.detectGit(path);
      if (mine !== this.checkRun || this.disposed) return;
      this.gitCheck = r;
      if (fill && r.ok && r.path && this.draft) this.draft.gitPath = r.path;
    } catch (e) {
      if (mine !== this.checkRun) return;
      const err = toApiError(e);
      this.gitCheck = { ok: false, source: path ? 'configured' : 'auto', path: null, version: null, problem: err.message };
    } finally {
      if (mine === this.checkRun) this.gitChecking = false;
    }
  }

  // ---- Reglas repo → perfil -----------------------------------------------------------------

  /** Motivo por el que `pattern` no se puede añadir, o `null` si vale. */
  ruleProblem(pattern: string, ignoreIndex = -1): RuleProblem | null {
    const n = normalizePattern(pattern);
    if (!n.ok) return n.reason;
    const dup = this.draft?.profileRules.some((r, i) => i !== ignoreIndex && r.pattern === n.pattern);
    return dup ? 'duplicate' : null;
  }

  addRule(pattern: string, profile: AiProfileKind): RuleProblem | null {
    const problem = this.ruleProblem(pattern);
    const n = normalizePattern(pattern);
    if (problem || !n.ok || !this.draft) return problem;
    this.draft.profileRules = [...this.draft.profileRules, { pattern: n.pattern, profile }];
    return null;
  }

  setRuleProfile(index: number, profile: AiProfileKind): void {
    if (!this.draft?.profileRules[index]) return;
    this.draft.profileRules = this.draft.profileRules.map((r, i): ProfileRule => (i === index ? { ...r, profile } : r));
  }

  removeRule(index: number): void {
    if (!this.draft) return;
    this.draft.profileRules = this.draft.profileRules.filter((_, i) => i !== index);
  }

  // ---- Guardar ---------------------------------------------------------------------------------

  /** Guarda el borrador. `true` si Rust lo aceptó. */
  async save(): Promise<boolean> {
    if (!this.draft || this.saving) return false;
    if (this.fontProblem) return false;
    this.saving = true;
    this.saveError = null;
    try {
      const view = await this.api.saveSettings(clone(this.draft));
      if (this.disposed) return true;
      this.view = view;
      this.draft = clone(view.settings);
      this.original = { theme: view.settings.theme, diffFontSize: view.settings.diffFontSize };
      globalSettings.applyExternal(view.settings.theme, view.settings.diffFontSize);
      this.gitCheck = null;
      void this.loadAi(); // las reglas pueden cambiar el perfil de la pestaña
      toasts.success(t('settings.saved'));
      return true;
    } catch (e) {
      this.saveError = toApiError(e);
      return false;
    } finally {
      this.saving = false;
    }
  }

  // ---- Inteligencia artificial (se aplica al momento) ------------------------------------------

  async patchAi(patch: AiSettingsPatch): Promise<boolean> {
    this.aiBusy = true;
    try {
      this.ai = await this.api.aiUpdateSettings({ ...patch });
      void this.loadEffective();
      return true;
    } catch (e) {
      toasts.error(t('ai.settingsFailed'), e);
      return false;
    } finally {
      this.aiBusy = false;
    }
  }
}

/**
 * Sincroniza tema y tamaño del diff con `settings.json` (Rust manda al arrancar; los cambios
 * hechos desde la barra de título o la paleta se guardan). Devuelve cómo dejar de sincronizar.
 */
export function startSettingsSync(api: Api): () => void {
  let stopped = false;
  let timer: ReturnType<typeof setTimeout> | null = null;

  const push = async (s: { theme: ThemePref; diffFontSize: number }) => {
    try {
      // Se parte de lo guardado ahora (el panel de Ajustes pudo cambiar otros campos).
      const current = (await api.getSettings()).settings;
      if (current.theme === s.theme && current.diffFontSize === s.diffFontSize) return;
      await api.saveSettings({ ...current, theme: s.theme, diffFontSize: s.diffFontSize });
    } catch {
      /* queda en localStorage; el siguiente arranque lo reintenta */
    }
  };

  void (async () => {
    try {
      const view = await api.getSettings();
      if (stopped) return;
      globalSettings.applyExternal(view.settings.theme, view.settings.diffFontSize);
      if (view.load.warning) toasts.warning(t('settings.loadWarning'), view.load.warning);
      globalSettings.onChange = (s) => {
        if (timer) clearTimeout(timer);
        timer = setTimeout(() => void push(s), 250);
      };
    } catch {
      /* sin Rust (o sin ajustes) la interfaz sigue con su copia local */
    }
  })();

  return () => {
    stopped = true;
    if (timer) clearTimeout(timer);
    globalSettings.onChange = null;
  };
}
