import {
  toApiError,
  type AiAccounts,
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

/** Nombre visible del perfil de IA. Se traduce (el `name` de Rust es «Personal»/«Empresa»). */
export function profileLabel(kind: 'personal' | 'empresa'): string {
  return t(kind === 'empresa' ? 'ai.dialog.profile.empresa' : 'ai.dialog.profile.personal');
}

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

/** Qué se rellenó y qué había antes, para «Deshacer». También es la propuesta vigente de la hoja. */
export interface AiNotice {
  provider: string;
  repoRevision: number;
  previous: { summary: string; description: string };
  /** La propuesta tal como llegó de Rust (la hoja puede editarla antes de «Usar»). */
  subject: string;
  body: string;
  commandSummary: string;
  /** Lo último que se escribió en los campos a partir de la propuesta. */
  applied: { subject: string; body: string };
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

  /** Hoja de propuesta abierta (Copiar, Usar, Regenerar, Descartar). */
  sheet = $state(false);
  regenerating = $state(false);
  sheetError = $state<string | null>(null);
  /** Cuenta que usará cada proveedor en el perfil de la pestaña (RF-IA12); solo se pide al abrir la hoja. */
  accounts = $state.raw<AiAccounts | null>(null);

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

  /** Los campos aún contienen la propuesta tal como se aplicó (no se tocaron desde entonces). */
  get inUse(): boolean {
    const n = this.notice;
    return !!n && this.tab.summary === n.applied.subject && this.tab.description === n.applied.body;
  }

  /** Cuenta (correo y organización) con la que respondió `provider`, si su CLI la expone. */
  accountLabel(provider: string): string | null {
    const a = this.accounts?.accounts.find((x) => x.provider === provider);
    if (!a || a.state !== 'loggedIn' || !a.email) return null;
    return a.organization ? `${a.email} · ${a.organization}` : a.email;
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
    this.sheet = false;
    this.sheetError = null;
    toasts.announce(t('ai.generating'));
    try {
      const s = await this.tab.api.aiGenerateCommitMessage(this.tab.id, {
        template: null,
        language: this.options.language,
        style: this.options.style
      });
      if (mine !== this.run || this.disposed) return false;
      const subject = s.subject.trim();
      const body = s.body.trim();
      this.notice = {
        provider: s.provider,
        repoRevision: s.repoRevision,
        previous: { summary: this.tab.summary, description: this.tab.description },
        subject,
        body,
        commandSummary: s.commandSummary,
        applied: { subject, body }
      };
      this.tab.setFields(subject, body);
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
    this.closeSheet();
    toasts.announce(t('ai.undone'));
  }

  dismissNotice(): void {
    this.notice = null;
    this.closeSheet();
  }

  // ---- Hoja de propuesta -----------------------------------------------------------------------

  openSheet(): void {
    if (!this.notice) return;
    this.sheet = true;
    this.sheetError = null;
    void this.loadAccounts();
  }

  /** Cierra la hoja; si estaba regenerando, cancela esa generación. */
  closeSheet(): void {
    if (this.regenerating) {
      this.run++;
      this.regenerating = false;
      void this.tab.api.aiCancel(this.tab.id).catch(() => undefined);
    }
    this.sheet = false;
    this.sheetError = null;
  }

  async loadAccounts(): Promise<void> {
    try {
      const a = await this.tab.api.aiAccounts({ tabId: this.tab.id });
      if (!this.disposed) this.accounts = a;
    } catch {
      /* sin cuenta que mostrar: la hoja dice «no disponible» */
    }
  }

  /** «Usar»: escribe lo (posiblemente editado) de la hoja en Resumen y Descripción. Nunca hace commit. */
  useProposal(value: { title: string; body: string }): void {
    const n = this.notice;
    if (!n) return;
    this.tab.setFields(value.title, value.body);
    this.notice = { ...n, applied: { subject: value.title, body: value.body } };
    this.fillTick++;
    this.sheet = false;
    toasts.announce(t('ai.used'));
  }

  /** «Descartar»: restaura lo que había antes de la propuesta y cierra la hoja. */
  discard(): void {
    const n = this.notice;
    if (!n) return;
    this.closeSheet();
    this.tab.setFields(n.previous.summary, n.previous.description);
    this.notice = null;
    toasts.announce(t('ai.discarded'));
  }

  /**
   * «Regenerar»: pide otra propuesta sin cerrar la hoja. Conserva el «antes» original (Deshacer
   * y Descartar siguen devolviendo el texto de antes de la IA) y solo reescribe los campos si
   * aún contenían la propuesta anterior: si el usuario los tocó, «Usar» decide.
   */
  async regenerate(): Promise<boolean> {
    const n = this.notice;
    if (!n || this.regenerating || this.generating || this.tab.committing) return false;
    const mine = ++this.run;
    this.regenerating = true;
    this.sheetError = null;
    toasts.announce(t('ai.generating'));
    try {
      const s = await this.tab.api.aiGenerateCommitMessage(this.tab.id, {
        template: null,
        language: this.options.language,
        style: this.options.style
      });
      if (mine !== this.run || this.disposed) return false;
      const subject = s.subject.trim();
      const body = s.body.trim();
      const wasInUse = this.inUse;
      this.notice = {
        ...n,
        provider: s.provider,
        repoRevision: s.repoRevision,
        subject,
        body,
        commandSummary: s.commandSummary,
        applied: wasInUse ? { subject, body } : n.applied
      };
      if (wasInUse) {
        this.tab.setFields(subject, body);
        this.fillTick++;
      }
      void this.loadState();
      void this.loadAccounts();
      return true;
    } catch (e) {
      if (mine !== this.run || this.disposed) return false;
      const err = toApiError(e);
      if (err.code !== 'CANCELLED') this.sheetError = err.message;
      return false;
    } finally {
      if (mine === this.run) this.regenerating = false;
    }
  }

  dismissError(): void {
    this.error = null;
  }

  /** Tras crear el commit o corregirlo, lo de la IA ya no aplica. */
  reset(): void {
    this.notice = null;
    this.error = null;
    this.sheet = false;
    this.sheetError = null;
  }

  providerName(id: string): string {
    return this.state?.providers.find((p) => p.id === id)?.displayName ?? id;
  }
}
