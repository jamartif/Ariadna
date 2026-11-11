<script lang="ts">
  import { untrack } from 'svelte';
  import { t } from '$lib/i18n';
  import { toasts } from '$lib/stores/toasts.svelte';
  import { focusTrap } from '$lib/util/focusTrap';
  import Icon from './Icon.svelte';

  /**
   * Hoja de propuesta reutilizable: la usan el mensaje de commit y, más adelante, las
   * descripciones de PR. Solo PRESENTA y devuelve lo que decide el usuario; no sabe de git.
   * La IA solo propone: nada se aplica hasta que se pulsa «Usar».
   */
  interface Props {
    kind: 'commit' | 'pr';
    /** Asunto (commit) o título (PR). */
    title: string;
    body: string;
    providerName: string;
    profileName?: string | null;
    /** Cuenta que usó el proveedor, si el CLI la expone. */
    account?: string | null;
    /** Resumen del comando lanzado (nunca incluye el prompt). */
    commandSummary?: string | null;
    /** La rama o el repo cambiaron mientras la IA pensaba. */
    stale?: boolean;
    /** Los campos ya contienen esta propuesta. */
    inUse?: boolean;
    regenerating?: boolean;
    /** Error de la última regeneración. */
    error?: string | null;
    onuse: (value: { title: string; body: string }) => void;
    onregenerate: () => void;
    ondiscard: () => void;
    onclose: () => void;
  }

  let { kind, title, body, providerName, profileName = null, account = null, commandSummary = null, stale = false, inUse = false, regenerating = false, error = null, onuse, onregenerate, ondiscard, onclose }: Props = $props();

  // Borrador local editable; una propuesta nueva (regenerar) lo sustituye.
  // Se parten de la propuesta ya en el primer pintado: el foco inicial cae en «Usar», que no puede
  // estar desactivado por un título vacío todavía sin copiar.
  let draftTitle = $state(untrack(() => title));
  let draftBody = $state(untrack(() => body));
  let seen = $state<string | null>(untrack(() => `${title}\u0000${body}`));
  $effect(() => {
    const key = `${title}\u0000${body}`;
    if (key !== seen) {
      seen = key;
      draftTitle = title;
      draftBody = body;
    }
  });

  const edited = $derived(draftTitle !== title || draftBody !== body);
  const emptyTitle = $derived(draftTitle.trim() === '');
  const text = $derived(draftBody.trim() === '' ? draftTitle.trim() : `${draftTitle.trim()}\n\n${draftBody.trim()}`);
  const titleLabel = $derived(kind === 'pr' ? t('ai.sheet.prTitle') : t('ai.sheet.subject'));
  const heading = $derived(kind === 'pr' ? t('ai.sheet.titlePr') : t('ai.sheet.titleCommit'));
  const uid = Math.random().toString(36).slice(2, 8);

  async function copy() {
    try {
      await navigator.clipboard.writeText(text);
      toasts.success(t('ai.sheet.copied'));
    } catch {
      toasts.warning(t('ai.sheet.copyFailed'));
    }
  }

  function use() {
    if (emptyTitle) return;
    onuse({ title: draftTitle.trim(), body: draftBody.trim() });
  }

  function key(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      e.stopPropagation();
      onclose();
    }
  }
</script>

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="backdrop" onclick={onclose}>
  <div class="dialog float" role="dialog" aria-modal="true" aria-labelledby="sheet-title-{uid}" aria-describedby="sheet-sub-{uid}" tabindex="-1" use:focusTrap onclick={(e) => e.stopPropagation()} onkeydown={key}>
    <header class="head">
      <span class="badge"><Icon name="sparkles" size={18} /></span>
      <div class="titles">
        <h2 id="sheet-title-{uid}">{heading}</h2>
        <p id="sheet-sub-{uid}">{t('ai.sheet.subtitle')}</p>
      </div>
      <button type="button" class="btn btn-ghost btn-icon" onclick={onclose} aria-label={t('ai.sheet.close')}>
        <Icon name="x" size={16} />
      </button>
    </header>

    <ul class="meta" aria-label={t('ai.sheet.provider', { provider: providerName })}>
      <li>{t('ai.sheet.provider', { provider: providerName })}</li>
      {#if profileName}<li>{t('ai.sheet.profile', { profile: profileName })}</li>{/if}
      <li class:muted={!account}>{account ? t('ai.sheet.account', { account }) : t('ai.sheet.accountUnknown')}</li>
    </ul>

    <div class="body">
      <div class="field-row">
        <label for="sheet-t-{uid}" class="lbl">{titleLabel}</label>
        <input id="sheet-t-{uid}" class="field" type="text" bind:value={draftTitle} disabled={regenerating} autocomplete="off" spellcheck="true" aria-invalid={emptyTitle ? 'true' : undefined} />
        {#if emptyTitle}<p class="warn" role="alert"><Icon name="triangle-alert" size={12} />{t('ai.sheet.empty')}</p>{/if}
      </div>
      <div class="field-row">
        <label for="sheet-b-{uid}" class="lbl">{t('ai.sheet.body')}</label>
        <textarea id="sheet-b-{uid}" class="field" rows="9" bind:value={draftBody} disabled={regenerating} spellcheck="true"></textarea>
      </div>
      {#if stale}<p class="warn"><Icon name="triangle-alert" size={12} />{t('ai.stale')}</p>{/if}
      {#if error}<p class="warn" role="alert"><Icon name="circle-alert" size={12} />{error}</p>{/if}
      {#if commandSummary}<p class="cmd mono" title={commandSummary}>{commandSummary}</p>{/if}
      {#if inUse && !edited}<p class="hint" role="status">{t('ai.sheet.inUse')}</p>{/if}
    </div>

    <footer class="foot">
      <button type="button" class="btn btn-ghost" onclick={ondiscard} disabled={regenerating}>{t('ai.sheet.discard')}</button>
      <span class="spacer"></span>
      <button type="button" class="btn" onclick={() => void copy()} disabled={emptyTitle}><Icon name="copy" size={14} />{t('ai.sheet.copy')}</button>
      <button type="button" class="btn" onclick={onregenerate} disabled={regenerating}>
        <Icon name="refresh-cw" size={14} spin={regenerating} />{regenerating ? t('ai.sheet.regenerating') : t('ai.sheet.regenerate')}
      </button>
      <button type="button" class="btn btn-primary" data-autofocus onclick={use} disabled={regenerating || emptyTitle}>
        <Icon name="check" size={14} />{inUse && !edited ? t('ai.sheet.usedAgain') : t('ai.sheet.use')}
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
    padding: 7vh var(--space-4) var(--space-4);
    background: var(--bg-overlay);
    backdrop-filter: blur(3px);
    animation: fade-in var(--dur) var(--ease);
    overflow: auto;
  }
  .dialog {
    width: min(620px, 100%);
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
  .meta li.muted {
    color: var(--text-subtle);
  }
  .body {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
    padding: var(--space-3) var(--space-4);
  }
  .field-row {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .lbl {
    font-size: var(--text-xs);
    font-weight: 600;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--text-muted);
  }
  textarea {
    padding: var(--space-2) var(--space-3);
    resize: vertical;
    line-height: 1.45;
  }
  .field[aria-invalid='true'] {
    border-color: var(--danger);
  }
  .warn {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: var(--text-xs);
    color: var(--warning);
  }
  .hint {
    font-size: var(--text-xs);
    color: var(--text-subtle);
  }
  .cmd {
    font-size: var(--text-xs);
    color: var(--text-subtle);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
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
</style>
