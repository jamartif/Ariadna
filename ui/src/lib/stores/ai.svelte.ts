import {
  toApiError,
  type AiLanguage,
  type AiState,
  type AiStyle,
  type AiSettingsPatch,
  type ApiError,
  type ContextPreview
} from '$lib/api/types';
import { t } from '$lib/i18n';
import { toasts } from './toasts.svelte';
import type { TabState } from './tab.svelte';

const OPTIONS_KEY = 'ariadna.ai.options.v1';

export interface AiOptions {
  language: AiLanguage;
  style: AiStyle;
}

export function loadAiOptions(): AiOptions {
  try {
    const o = JSON.parse(localStorage.getItem(OPTIONS_KEY) ?? '{}') as Partial<AiOptions>;
    return {
      language: o.language === 'es' || o.language === 'en' ? o.language : 'auto',
      style: o.style === 'conventional' ? 'conventional' : 'plain'
    };
  } catch {
    return { language: 'auto', style: 'plain' };
  }
}

/** Qué se rellenó y qué había antes, para «Deshacer». */
export interface AiNotice {
  provider: string;
  repoRevision: number;
  previous: { summary: string; description: string };
}

/**
 * Generación de mensajes de commit con IA para UNA pestaña. La IA solo propone:
 * rellena Resumen y Descripción y deja siempre el commit en manos del usuario. La UI
 * no envía ningún diff; `ai_generate_commit_message` lo lee Rust (lo preparado).
 */
export class AiController {
  state = $state.raw<AiState | null>(null);
  status = $state<'idle' | 'generating'>('idle');
  error = $state.raw<ApiError | null>(null);
  notice = $state.raw<AiNotice | null>(null);
  options = $state<AiOptions>(loadAiOptions());
  /** Cambia en cada relleno: la interfaz lo usa para reanimar los campos. */
  fillTick = $state(0);

  preview = $state.raw<ContextPreview | null>(null);
  previewLoading = $state(false);
  previewError = $state.raw<ApiError | null>(null);

  private run = 0;
  private disposed = false;

  constructor(private readonly tab: TabState) {}

  dispose(): void {
    this.disposed = true;
    this.run++;
  }

  get generating(): boolean {
    return this.status === 'generating';
  }

  /** ¿La propuesta puede estar obsoleta? El repo cambió desde que Rust leyó lo preparado. */
  get stale(): boolean {
    return this.notice !== null && this.tab.revision !== this.notice.repoRevision;
  }

  /** Proveedor que se usaría ahora (Auto = el primero habilitado y disponible). */
  get activeProvider() {
    const s = this.state;
    if (!s) return null;
    if (s.choice !== 'auto') return s.providers.find((p) => p.id === s.choice) ?? null;
    return s.providers.find((p) => p.enabled && p.availability === 'available') ?? s.providers.find((p) => p.enabled) ?? null;
  }

  get needsConsent(): boolean {
    const p = this.activeProvider;
    return !!p && p.availability === 'available' && !p.consented;
  }

  async loadState(): Promise<void> {
    try {
      const s = await this.tab.api.aiState(this.tab.id);
      if (!this.disposed) this.state = s;
    } catch {
      /* la IA es opcional: sin estado el botón sigue funcionando y Rust responderá con el error */
    }
  }

  private setOptions(patch: Partial<AiOptions>): void {
    this.options = { ...this.options, ...patch };
    try {
      localStorage.setItem(OPTIONS_KEY, JSON.stringify(this.options));
    } catch {
      /* ignorar */
    }
  }

  setLanguage(language: AiLanguage): void {
    this.setOptions({ language });
  }

  setStyle(style: AiStyle): void {
    this.setOptions({ style });
  }

  async setChoice(choice: string): Promise<void> {
    await this.update({ choice });
  }

  async setProfile(profile: 'personal' | 'empresa'): Promise<void> {
    await this.update({ profile });
  }

  async setEnabled(enabled: boolean): Promise<void> {
    await this.update({ enabled });
    if (enabled) this.error = null;
  }

  /** Guarda varios ajustes de una vez (diálogo de consentimiento). `true` si se guardó. */
  async saveSettings(patch: AiSettingsPatch): Promise<boolean> {
    try {
      this.state = await this.tab.api.aiUpdateSettings(patch);
      this.preview = null;
      this.error = null; // lo que falló antes puede ya no aplicar con los ajustes nuevos
      return true;
    } catch (e) {
      toasts.error(t('ai.settingsFailed'), e);
      return false;
    }
  }

  private async update(patch: AiSettingsPatch): Promise<void> {
    try {
      this.state = await this.tab.api.aiUpdateSettings(patch);
      this.preview = null;
    } catch (e) {
      toasts.error(t('ai.settingsFailed'), e);
    }
  }

  async loadPreview(): Promise<void> {
    this.previewLoading = true;
    this.previewError = null;
    try {
      const p = await this.tab.api.aiPreviewContext(this.tab.id);
      if (!this.disposed) this.preview = p;
    } catch (e) {
      this.previewError = toApiError(e);
    } finally {
      this.previewLoading = false;
    }
  }

  /** Genera y RELLENA los campos. Nunca crea el commit. */
  async generate(): Promise<boolean> {
    if (this.generating || this.tab.committing) return false;
    const mine = ++this.run;
    this.status = 'generating';
    this.error = null;
    this.notice = null;
    toasts.announce(t('ai.generating'));
    try {
      const s = await this.tab.api.aiGenerateCommitMessage(this.tab.id, {
        template: null,
        language: this.options.language,
        style: this.options.style
      });
      if (mine !== this.run || this.disposed) return false;
      this.notice = {
        provider: s.provider,
        repoRevision: s.repoRevision,
        previous: { summary: this.tab.summary, description: this.tab.description }
      };
      this.tab.setFields(s.subject.trim(), s.body.trim());
      this.fillTick++;
      toasts.announce(t('ai.filled', { provider: this.providerName(s.provider) }));
      void this.loadState();
      return true;
    } catch (e) {
      if (mine !== this.run || this.disposed) return false;
      const err = toApiError(e);
      if (err.code !== 'CANCELLED') this.error = err;
      return false;
    } finally {
      if (mine === this.run) this.status = 'idle';
    }
  }

  async cancel(): Promise<void> {
    if (!this.generating) return;
    this.run++; // lo que llegue después se ignora
    this.status = 'idle';
    toasts.announce(t('ai.cancelled'));
    try {
      await this.tab.api.aiCancel(this.tab.id);
    } catch {
      /* ya había terminado */
    }
  }

  /** Restaura lo que había antes de rellenar. */
  undo(): void {
    const n = this.notice;
    if (!n) return;
    this.tab.setFields(n.previous.summary, n.previous.description);
    this.notice = null;
    toasts.announce(t('ai.undone'));
  }

  dismissNotice(): void {
    this.notice = null;
  }

  dismissError(): void {
    this.error = null;
  }

  /** Tras crear el commit o corregirlo, lo de la IA ya no aplica. */
  reset(): void {
    this.notice = null;
    this.error = null;
  }

  providerName(id: string): string {
    return this.state?.providers.find((p) => p.id === id)?.displayName ?? id;
  }
}
