<script lang="ts">
  import type { AiCommitReport, Commit } from '$lib/api/types';
  import { toApiError } from '$lib/api/types';
  import { t } from '$lib/i18n';
  import type { TabState } from '$lib/stores/tab.svelte';
  import { toasts } from '$lib/stores/toasts.svelte';
  import { focusTrap } from '$lib/util/focusTrap';
  import Icon from './Icon.svelte';
  import MarkdownView from './MarkdownView.svelte';

  /**
   * Informe de cambios de un commit: la IA propone un Markdown con qué cambia a nivel
   * funcional, riesgos, posibles bugs y qué probar. Se pinta en el visor y solo se escribe
   * en el repositorio (`informes-cambios/`) si el usuario pulsa «Guardar».
   */
  let { tab, commit, onclose }: { tab: TabState; commit: Commit; onclose: () => void } = $props();

  let report = $state<AiCommitReport | null>(null);
  let error = $state<string | null>(null);
  let busy = $state(true);
  let saving = $state(false);
  let fileName = $state('');
  let savedTo = $state<string | null>(null);
  /**
   * Tope de contexto con el que se intentó el informe cuando falló por tamaño. Si tiene valor, el
   * aviso muestra el ajuste concreto («Tamaño de contexto», en Ajustes › IA) y ofrece reintentar con
   * más sin salir del diálogo.
   */
  let contextLimit = $state(0);

  /** Tope máximo aceptado por el núcleo (ver `MAX_CONTEXT_BYTES` en `core/ai.rs`). */
  const MAX_CONTEXT_BYTES = 4_000_000;

  const nextLimit = $derived(Math.min(MAX_CONTEXT_BYTES, Math.max(200_000, contextLimit * 2)));

  void start();

  function formatBytes(bytes: number): string {
    return bytes >= 1_000_000
      ? `${(bytes / 1_000_000).toLocaleString('es-ES', { maximumFractionDigits: 1 })} MB`
      : `${Math.round(bytes / 1000).toLocaleString('es-ES')} kB`;
  }

  async function start(): Promise<void> {
    busy = true;
    error = null;
    try {
      report = await tab.api.aiCommitReport(tab.id, commit.sha);
      fileName = `${commit.sha.slice(0, 7)}-informe.md`;
      contextLimit = 0;
    } catch (e) {
      const api = toApiError(e);
      error = api.message;
      // El tope viene en el detalle como «bytes/max»; sirve para decir cuál es el ajuste actual.
      const parsed = api.code === 'AI_CONTEXT_TOO_LARGE' ? Number(api.detail?.split('/')[1]) : NaN;
      contextLimit = Number.isFinite(parsed) && parsed > 0 ? parsed : 0;
      if (api.code === 'AI_CONTEXT_TOO_LARGE' && contextLimit === 0) {
        contextLimit = 100_000;
      }
    } finally {
      busy = false;
    }
  }

  /**
   * Sube el ajuste «Tamaño de contexto» (Ajustes › IA) y vuelve a pedir el informe, sin cerrar el
   * diálogo. El ajuste queda guardado: el siguiente informe ya usa el tope nuevo.
   */
  async function retryWithMoreContext(): Promise<void> {
    const target = nextLimit;
    try {
      await tab.api.aiUpdateSettings({ tabId: tab.id, maxContextBytes: target });
      contextLimit = target;
      await start();
    } catch (e) {
      toasts.error(e instanceof Error ? e.message : String(e), e);
    }
  }

  async function save(): Promise<void> {
    if (!report || saving) return;
    saving = true;
    try {
      const saved = await tab.api.saveCommitReport(tab.id, fileName, report.markdown);
      savedTo = saved.path;
      toasts.success(t('report.saved', { path: saved.path }));
      void tab.revision;
      void refreshFiles();
    } catch (e) {
      toasts.error(e instanceof Error ? e.message : String(e), e);
    } finally {
      saving = false;
    }
  }

  /** Tras guardar, el archivo nuevo aparece en «Cambios»: refresca el estado del repo. */
  async function refreshFiles(): Promise<void> {
    try {
      await tab.refresh();
    } catch {
      // El refresco es cosmético: si falla, el informe ya está guardado.
    }
  }

  async function copy(): Promise<void> {
    if (!report) return;
    try {
      await navigator.clipboard.writeText(report.markdown);
      toasts.success(t('report.copied'));
    } catch {
      toasts.error(t('report.copyFailed'), null);
    }
  }

  function key(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      e.preventDefault();
      e.stopPropagation();
      onclose();
    }
  }
</script>

<svelte:window onkeydown={key} />

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="backdrop" onclick={onclose}>
  <div
    class="dialog float"
    role="dialog"
    aria-modal="true"
    aria-labelledby="report-dlg-title"
    tabindex="-1"
    data-testid="report-dialog"
    use:focusTrap
    onclick={(e) => e.stopPropagation()}
  >
    <header class="head">
      <span class="badge"><Icon name="sparkles" size={16} /></span>
      <h2 id="report-dlg-title">{t('report.title')}</h2>
      <button type="button" class="btn btn-ghost btn-icon" aria-label={t('report.close')} onclick={onclose}>
        <Icon name="x" size={14} />
      </button>
    </header>

    <p class="subject mono" title={commit.sha}>{commit.sha.slice(0, 7)} · {commit.subject}</p>

    {#if busy}
      <p class="state" role="status" data-testid="report-busy">
        <Icon name="loader" size={14} spin />{t('report.generating')}
      </p>
    {:else if error}
      <div class="state bad" role="alert">
        <p>{error}</p>
        {#if contextLimit}
          <p class="hint" data-testid="report-context-hint">
            {t('report.tooLargeHint', { limit: formatBytes(contextLimit) })}
          </p>
        {/if}
        <div class="actions">
          <button type="button" class="btn btn-sm" onclick={() => void start()}>{t('report.retry')}</button>
          {#if contextLimit}
            <button
              type="button"
              class="btn btn-sm btn-primary"
              data-testid="report-more-context"
              disabled={nextLimit > MAX_CONTEXT_BYTES}
              onclick={() => void retryWithMoreContext()}
            >
              <Icon name="sparkles" size={13} />{t('report.moreContext', { limit: formatBytes(nextLimit) })}
            </button>
          {/if}
        </div>
      </div>
    {:else if report}
      {#if report.truncated}
        <p class="hint" role="note">{t('report.truncated')}</p>
      {/if}
      <div class="body" data-testid="report-body">
        <MarkdownView source={report.markdown} />
      </div>
      <p class="meta">
        {t('report.meta', { provider: report.provider, files: report.filesChanged })}
      </p>
      <div class="save">
        <label class="fieldwrap">
          <span>{t('report.fileName')}</span>
          <input
            class="field mono"
            type="text"
            maxlength={96}
            bind:value={fileName}
            aria-label={t('report.fileName')}
            data-testid="report-file-name"
          />
        </label>
        <div class="actions">
          <button type="button" class="btn btn-sm" onclick={() => void copy()}>{t('report.copy')}</button>
          <button
            type="button"
            class="btn btn-sm primary"
            disabled={saving}
            onclick={() => void save()}
            data-testid="report-save"
          >
            <Icon name="download" size={13} />{saving ? t('report.saving') : t('report.save')}
          </button>
        </div>
      </div>
      {#if savedTo}
        <p class="hint ok" role="status" data-testid="report-saved">{t('report.saved', { path: savedTo })}</p>
      {:else}
        <p class="hint">{t('report.saveHint')}</p>
      {/if}
    {/if}
  </div>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 60;
    display: grid;
    place-items: center;
    padding: var(--space-4);
    background: color-mix(in srgb, var(--bg-app) 55%, transparent);
    backdrop-filter: blur(2px);
  }
  .dialog {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
    width: min(680px, 100%);
    max-height: min(86vh, 760px);
    overflow: auto;
    padding: var(--space-4);
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    background: var(--bg-surface);
    box-shadow: var(--elev-3);
  }
  .head {
    display: flex;
    align-items: center;
    gap: var(--space-2);
  }
  .head h2 {
    flex: 1;
    margin: 0;
    font-size: var(--text-md);
  }
  .badge {
    display: grid;
    place-items: center;
    width: 28px;
    height: 28px;
    border-radius: var(--radius-sm);
    background: var(--accent-soft);
    color: var(--accent);
  }
  .subject {
    margin: 0;
    font-size: var(--text-xs);
    color: var(--text-muted);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .state {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    font-size: var(--text-sm);
    color: var(--text-muted);
  }
  .state.bad {
    flex-direction: column;
    align-items: flex-start;
    color: var(--danger);
  }
  .body {
    padding: var(--space-3);
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    background: var(--bg-subtle);
    font-size: var(--text-sm);
    overflow: auto;
  }
  .meta {
    margin: 0;
    font-size: var(--text-xs);
    color: var(--text-muted);
  }
  .save {
    display: flex;
    flex-wrap: wrap;
    align-items: flex-end;
    gap: var(--space-2);
  }
  .fieldwrap {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
    flex: 1;
    min-width: 12rem;
    font-size: var(--text-xs);
    color: var(--text-muted);
  }
  .field {
    font-size: var(--text-sm);
    padding: var(--space-1) var(--space-2);
  }
  .actions {
    display: flex;
    gap: var(--space-2);
  }
  .hint {
    margin: 0;
    font-size: var(--text-xs);
    color: var(--text-muted);
  }
  .hint.ok {
    color: var(--success, var(--accent));
  }
</style>
