<script lang="ts">
  import { t } from '$lib/i18n';
  import { diffStats, lineDiff } from '$lib/util/lineDiff';
  import { focusTrap } from '$lib/util/focusTrap';
  import Icon from './Icon.svelte';

  /**
   * Panel de la propuesta de la IA para un conflicto: el archivo completo que propone, la
   * diferencia contra el resultado actual y su justificación. Solo PRESENTA y devuelve lo que
   * decide el usuario; no escribe en disco (eso lo hace «Marcar como resuelto», con su
   * confirmación nativa).
   */
  interface Props {
    /** Ruta del archivo (ya en texto, para mostrar). */
    path: string;
    /** Lo que hay ahora en el editor; la propuesta se compara contra esto. */
    current: string;
    /** Contenido completo propuesto. */
    content: string;
    rationale: string | null;
    providerName: string;
    /** Resumen del comando lanzado (nunca incluye el prompt). */
    commandSummary?: string | null;
    regenerating?: boolean;
    /** Error de la última regeneración. */
    error?: string | null;
    onuse: () => void;
    onregenerate: () => void;
    ondiscard: () => void;
    onclose: () => void;
  }

  let {
    path,
    current,
    content,
    rationale,
    providerName,
    commandSummary = null,
    regenerating = false,
    error = null,
    onuse,
    onregenerate,
    ondiscard,
    onclose
  }: Props = $props();

  const rows = $derived(lineDiff(current, content));
  const stats = $derived(diffStats(rows));
  const unchanged = $derived(stats.additions === 0 && stats.deletions === 0);
  const uid = Math.random().toString(36).slice(2, 8);

  function key(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      e.stopPropagation();
      if (!regenerating) onclose();
    }
  }
</script>

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="backdrop" onclick={() => !regenerating && onclose()}>
  <div
    class="dialog float"
    role="dialog"
    aria-modal="true"
    aria-labelledby="aic-title-{uid}"
    aria-describedby="aic-sub-{uid}"
    tabindex="-1"
    use:focusTrap
    onclick={(e) => e.stopPropagation()}
    onkeydown={key}
  >
    <header class="head">
      <span class="badge"><Icon name="sparkles" size={18} /></span>
      <div class="titles">
        <h2 id="aic-title-{uid}">{t('conflicts.ai.title')}</h2>
        <p id="aic-sub-{uid}">{t('conflicts.ai.subtitle', { path })}</p>
      </div>
      <button type="button" class="btn btn-ghost btn-icon" onclick={onclose} disabled={regenerating} aria-label={t('conflicts.ai.close')}>
        <Icon name="x" size={16} />
      </button>
    </header>

    <ul class="meta">
      <li>{t('ai.sheet.provider', { provider: providerName })}</li>
      {#if commandSummary}<li class="mono cmd" title={commandSummary}>{commandSummary}</li>{/if}
    </ul>

    <div class="body">
      {#if rationale}
        <section class="why" aria-labelledby="aic-why-{uid}">
          <h3 id="aic-why-{uid}">{t('conflicts.ai.rationale')}</h3>
          <p>{rationale}</p>
        </section>
      {/if}

      <div class="stats">
        <span class="add">+{stats.additions}</span>
        <span class="del">−{stats.deletions}</span>
        <span class="legend">{t('conflicts.ai.diffLegend')}</span>
      </div>

      {#if unchanged}
        <p class="hint" role="status">{t('conflicts.ai.noChanges')}</p>
      {:else}
        <ol class="diff mono" aria-label={t('conflicts.ai.diffLabel')}>
          {#each rows as row, i (i)}
            {#if row.kind === 'gap'}
              <li class="gap" aria-hidden="true">···</li>
            {:else}
              <li class={row.kind} data-kind={row.kind}>
                <span class="sign" aria-hidden="true">{row.kind === 'add' ? '+' : row.kind === 'del' ? '−' : ' '}</span>
                <span class="sr-only">{row.kind === 'add' ? t('conflicts.ai.added') : row.kind === 'del' ? t('conflicts.ai.removed') : ''}</span>
                <span class="tx">{row.text === '' ? ' ' : row.text}</span>
              </li>
            {/if}
          {/each}
        </ol>
      {/if}

      {#if error}<p class="warn" role="alert"><Icon name="circle-alert" size={12} />{error}</p>{/if}
      <p class="hint">{t('conflicts.ai.notApplied')}</p>
    </div>

    <footer class="foot">
      <button type="button" class="btn btn-ghost" onclick={ondiscard} disabled={regenerating}>{t('conflicts.ai.discard')}</button>
      <span class="spacer"></span>
      <button type="button" class="btn" onclick={onregenerate} disabled={regenerating}>
        <Icon name="refresh-cw" size={14} spin={regenerating} />
        {regenerating ? t('conflicts.ai.regenerating') : t('conflicts.ai.regenerate')}
      </button>
      <button type="button" class="btn btn-primary" data-autofocus onclick={onuse} disabled={regenerating}>
        <Icon name="check" size={14} />{t('conflicts.ai.use')}
      </button>
    </footer>
  </div>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 90;
    display: flex;
    align-items: flex-start;
    justify-content: center;
    padding: 6vh var(--space-4) var(--space-4);
    background: var(--bg-overlay);
    backdrop-filter: blur(3px);
    animation: fade-in var(--dur) var(--ease);
    overflow: auto;
  }
  .dialog {
    width: min(760px, 100%);
    max-height: 88vh;
    display: flex;
    flex-direction: column;
    border-radius: 16px;
    overflow: hidden;
  }
  .head {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    padding: var(--space-4) var(--space-4) var(--space-2);
  }
  .badge {
    display: grid;
    place-items: center;
    width: 36px;
    height: 36px;
    flex: none;
    border-radius: 11px;
    background: var(--grad-accent-soft);
    border: 1px solid color-mix(in srgb, var(--accent) 35%, transparent);
    color: var(--accent);
  }
  .titles {
    flex: 1;
    min-width: 0;
  }
  h2 {
    font-size: var(--text-lg);
    font-weight: 650;
    letter-spacing: -0.015em;
  }
  .titles p {
    color: var(--text-muted);
    font-size: var(--text-sm);
    overflow-wrap: anywhere;
  }
  .meta {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    margin: 0;
    padding: 0 var(--space-4);
    list-style: none;
  }
  .meta li {
    padding: 2px 8px;
    border-radius: 999px;
    border: 1px solid var(--border);
    background: var(--bg-subtle);
    font-size: var(--text-xs);
    color: var(--text-muted);
  }
  .meta li.cmd {
    max-width: 100%;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .body {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
    min-height: 0;
    padding: var(--space-3) var(--space-4);
    overflow: auto;
  }
  .why h3 {
    margin: 0 0 2px;
    font-size: var(--text-xs);
    font-weight: 600;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--text-muted);
  }
  .why p {
    margin: 0;
    font-size: var(--text-sm);
    color: var(--text);
  }
  .stats {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    font-size: var(--text-xs);
  }
  .stats .add {
    color: var(--success);
    font-weight: 600;
  }
  .stats .del {
    color: var(--danger);
    font-weight: 600;
  }
  .legend {
    color: var(--text-subtle);
  }
  .diff {
    margin: 0;
    padding: 0;
    list-style: none;
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    background: var(--bg-surface);
    font-size: var(--text-xs);
    line-height: var(--code-lh, 1.5);
    overflow: auto;
    max-height: 46vh;
  }
  .diff li {
    display: flex;
    gap: var(--space-2);
    padding: 0 var(--space-2);
    white-space: pre;
  }
  .diff li.add {
    background: var(--success-soft);
  }
  .diff li.del {
    background: var(--danger-soft);
  }
  .diff li.gap {
    color: var(--text-subtle);
    padding-left: var(--space-3);
    letter-spacing: 0.2em;
  }
  .sign {
    flex: none;
    width: 1ch;
    color: var(--text-subtle);
  }
  .diff li.add .sign {
    color: var(--success);
  }
  .diff li.del .sign {
    color: var(--danger);
  }
  .tx {
    min-width: 0;
  }
  .warn {
    display: flex;
    align-items: center;
    gap: 6px;
    margin: 0;
    font-size: var(--text-xs);
    color: var(--danger);
  }
  .hint {
    margin: 0;
    font-size: var(--text-xs);
    color: var(--text-subtle);
  }
  .foot {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: var(--space-2);
    padding: var(--space-3) var(--space-4) var(--space-4);
    border-top: 1px solid var(--border);
  }
  .spacer {
    flex: 1;
  }
  .sr-only {
    position: absolute;
    width: 1px;
    height: 1px;
    padding: 0;
    margin: -1px;
    overflow: hidden;
    clip-path: inset(50%);
    white-space: nowrap;
  }
</style>
