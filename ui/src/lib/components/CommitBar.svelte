<script lang="ts">
  import { onMount } from 'svelte';
  import { t, tn } from '$lib/i18n';
  import { formatCombo, isMac, registerHandlers, SHORTCUTS } from '$lib/shortcuts/registry';
  import type { TabState } from '$lib/stores/tab.svelte';
  import { profileLabel } from '$lib/stores/ai.svelte';
  import { toasts } from '$lib/stores/toasts.svelte';
  import AiConsentDialog from './AiConsentDialog.svelte';
  import Icon from './Icon.svelte';
  import ProposalSheet from './ProposalSheet.svelte';

  let { tab }: { tab: TabState } = $props();

  const SUBJECT_LIMIT = 72;

  let summaryInput = $state<HTMLInputElement>();
  let textarea = $state<HTMLTextAreaElement>();
  let canAmend = $state(true);
  let amendReason = $state<string | null>(null);
  let aiDialog = $state(false);
  let cancelBtn = $state<HTMLButtonElement>();
  let aiBtn = $state<HTMLButtonElement>();
  let hadFocus = false;

  const comboOf = (id: string, fallback: string) => formatCombo(SHORTCUTS.find((s) => s.id === id)?.combo ?? fallback, isMac());
  const submitCombo = $derived(comboOf('commit.submit', 'Mod+Enter'));
  const aiCombo = $derived(comboOf('ai.generate', 'Mod+Shift+G'));

  const ai = $derived(tab.ai);
  const subjectLen = $derived([...tab.summary].length);
  const overLimit = $derived(subjectLen > SUBJECT_LIMIT);
  const empty = $derived(tab.message.trim() === '');
  const nothingStaged = $derived(tab.stagedCount === 0 && !tab.amend);
  const blockedReason = $derived(
    tab.committing ? null : empty ? t('commit.reasonEmpty') : nothingStaged ? t('commit.reasonNothing') : null
  );
  const canSubmit = $derived(!tab.committing && !empty && !nothingStaged && !ai.generating);
  const detached = $derived(tab.snapshot?.status.isDetached === true);

  // La IA describe lo PREPARADO: sin nada preparado no hay qué enviar.
  const aiBlocked = $derived(tab.stagedCount === 0 ? t('ai.nothingStaged') : tab.committing ? t('ai.busyTitle') : null);
  const aiDisabled = $derived(ai.state?.enabled === false);
  const aiTitle = $derived(aiBlocked ?? t('ai.generateTitle', { shortcut: aiCombo }));
  const notice = $derived(ai.notice);
  const err = $derived(ai.error);
  const needsSettings = $derived(err?.code === 'AI_DISABLED' || err?.code === 'AI_CONSENT_REQUIRED');
  const needsCli = $derived(err?.code === 'AI_NOT_INSTALLED' || err?.code === 'AI_NOT_LOGGED_IN');

  // ¿Se puede corregir el último commit? (amend_draft.canAmend)
  $effect(() => {
    void tab.revision;
    let alive = true;
    tab.api
      .amendDraft(tab.id)
      .then((d) => {
        if (!alive) return;
        canAmend = d.canAmend;
        amendReason = d.reason;
      })
      .catch(() => {
        if (alive) canAmend = false;
      });
    return () => {
      alive = false;
    };
  });

  // Al empezar, el foco pasa a «Cancelar» (el botón que tenía el foco desaparece y Esc dejaría
  // de llegar al formulario); al terminar vuelve al botón de IA si el foco se había perdido.
  $effect(() => {
    if (ai.generating) {
      hadFocus = true;
      queueMicrotask(() => cancelBtn?.focus());
    } else if (hadFocus) {
      hadFocus = false;
      queueMicrotask(() => {
        const a = document.activeElement;
        if (!a || a === document.body) aiBtn?.focus();
      });
    }
  });

  async function submit() {
    if (!canSubmit) return;
    await tab.submit();
  }

  /** Botón, atajo y paleta pasan por aquí. Devuelve `true` si el atajo queda atendido. */
  function requestAi(): boolean {
    if (ai.generating) return true;
    if (aiBlocked) {
      toasts.info(t('ai.generate'), aiBlocked);
      return true;
    }
    if (aiDisabled) {
      aiDialog = true;
      return true;
    }
    void ai.generate();
    return true;
  }

  function summaryKey(e: KeyboardEvent) {
    // En una línea: Intro pasa a la descripción (no envía el formulario).
    if (e.key === 'Enter' && !e.ctrlKey && !e.metaKey && !e.shiftKey && !e.altKey && !e.isComposing) {
      e.preventDefault();
      textarea?.focus();
    }
  }

  function summaryPaste(e: ClipboardEvent) {
    const text = e.clipboardData?.getData('text') ?? '';
    if (!/[\r\n]/.test(text)) return;
    e.preventDefault();
    // Un mensaje completo pegado en un campo vacío se reparte entre asunto y descripción.
    if (tab.summary === '' && tab.description === '') tab.setMessage(text.replace(/\r\n?/g, '\n'));
    else tab.setSummary(tab.summary + (text.split(/\r?\n/).find((l) => l.trim() !== '') ?? ''));
  }

  function formKey(e: KeyboardEvent) {
    if (e.key === 'Escape' && ai.generating) {
      e.preventDefault();
      e.stopPropagation();
      void ai.cancel();
    }
  }

  onMount(() => {
    void ai.loadState();
    return registerHandlers({
      'commit.submit': () => {
        if (!canSubmit) return false;
        void submit();
        return true;
      },
      'commit.focus': () => {
        summaryInput?.focus();
        return true;
      },
      'ai.generate': requestAi
    });
  });
</script>

<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<form
  class="commit"
  aria-label={t('commit.region')}
  aria-busy={ai.generating}
  onsubmit={(e) => {
    e.preventDefault();
    void submit();
  }}
  onkeydown={formKey}
>
  <div class="head">
    <label for="commit-summary-{tab.id}" class="lbl">{t('commit.summaryLabel')}</label>
    <span class="counter" class:over={overLimit} id="subject-count-{tab.id}">
      {#if overLimit}<Icon name="triangle-alert" size={12} />{/if}
      {t('commit.subjectCount', { n: subjectLen, max: SUBJECT_LIMIT })}
    </span>
  </div>

  <div class="subject-row">
    <input
      id="commit-summary-{tab.id}"
      bind:this={summaryInput}
      type="text"
      class="field"
      data-scope="editor"
      placeholder={tab.amend ? t('commit.placeholderAmend') : t('commit.summaryPlaceholder')}
      value={tab.summary}
      oninput={(e) => tab.setSummary(e.currentTarget.value)}
      onkeydown={summaryKey}
      onpaste={summaryPaste}
      aria-describedby="subject-count-{tab.id} commit-hint-{tab.id}"
      aria-invalid={overLimit ? 'true' : undefined}
      autocomplete="off"
      spellcheck="true"
      disabled={tab.committing}
      readonly={ai.generating}
    />
    {#if ai.generating}
      <span class="ai-busy" role="status">
        <Icon name="loader" size={16} spin /><span class="sr-only">{t('ai.generatingBtn')}</span>
      </span>
      <button type="button" class="btn ai-cancel" bind:this={cancelBtn} onclick={() => void ai.cancel()}>
        {t('ai.cancel')}<kbd class="kbd">Esc</kbd>
      </button>
    {:else}
      <button
        type="button"
        class="btn ai-btn"
        bind:this={aiBtn}
        class:is-disabled={!!aiBlocked}
        aria-disabled={aiBlocked ? 'true' : undefined}
        aria-keyshortcuts="Control+Shift+G Meta+Shift+G"
        aria-describedby={aiBlocked ? `ai-reason-${tab.id}` : undefined}
        title={aiTitle}
        onclick={requestAi}
      >
        <Icon name="sparkles" size={14} />{t('ai.generate')}
      </button>
      {#if aiBlocked}<span id="ai-reason-{tab.id}" class="sr-only">{aiBlocked}</span>{/if}
    {/if}
  </div>

  {#if notice && !ai.generating}
    <div class="strip ok" role="status">
      <Icon name="sparkles" size={14} />
      <span class="msg">{t('ai.proposedBy', { provider: ai.providerName(notice.provider) })}</span>
      <button type="button" class="btn btn-ghost btn-sm" onclick={() => ai.openSheet()}>{t('ai.viewProposal')}</button>
      <button type="button" class="btn btn-ghost btn-sm" onclick={() => ai.undo()}>{t('ai.undo')}</button>
      <button type="button" class="btn btn-ghost btn-icon btn-sm" aria-label={t('ai.dismiss')} onclick={() => ai.dismissNotice()}>
        <Icon name="x" size={12} />
      </button>
    </div>
    {#if ai.stale}
      <p class="warn"><Icon name="triangle-alert" size={12} />{t('ai.stale')}</p>
    {/if}
  {/if}

  {#if err && !ai.generating}
    <div class="strip bad" role="alert">
      <Icon name="circle-alert" size={14} />
      <div class="msg">
        <strong>{t('ai.errorTitle')}</strong>
        <span>{err.message}</span>
        {#if needsCli && err.detail}<code class="cmd">{t('ai.runCommand', { command: err.detail })}</code>{/if}
      </div>
      {#if needsSettings || needsCli}
        <button type="button" class="btn btn-sm" onclick={() => (aiDialog = true)}>{t('ai.openSettings')}</button>
      {:else}
        <button type="button" class="btn btn-sm" onclick={requestAi}>{t('ai.retry')}</button>
      {/if}
      <button type="button" class="btn btn-ghost btn-icon btn-sm" aria-label={t('ai.dismiss')} onclick={() => ai.dismissError()}>
        <Icon name="x" size={12} />
      </button>
    </div>
  {:else if aiDisabled && !ai.generating}
    <div class="strip info" role="status">
      <Icon name="info" size={14} />
      <div class="msg">
        <strong>{t('ai.disabledTitle')}</strong>
        <span>{t('ai.disabledBody')}</span>
      </div>
      <button type="button" class="btn btn-sm" onclick={() => (aiDialog = true)}>{t('ai.enable')}</button>
    </div>
  {/if}

  <label for="commit-desc-{tab.id}" class="lbl desc-lbl">{t('commit.descriptionLabel')}</label>
  <textarea
    id="commit-desc-{tab.id}"
    bind:this={textarea}
    class="field"
    rows="4"
    data-scope="editor"
    placeholder={t('commit.descriptionPlaceholder')}
    value={tab.description}
    oninput={(e) => tab.setDescription(e.currentTarget.value)}
    aria-describedby="commit-hint-{tab.id}"
    spellcheck="true"
    disabled={tab.committing}
    readonly={ai.generating}
  ></textarea>

  <div id="commit-hint-{tab.id}" class="hints">
    {#if overLimit}
      <p class="warn"><Icon name="triangle-alert" size={12} />{t('commit.subjectTooLong', { max: SUBJECT_LIMIT })}</p>
    {/if}
    {#if detached && !tab.amend}
      <p class="warn"><Icon name="triangle-alert" size={12} />{t('commit.detached')}</p>
    {/if}
    {#if ai.generating}
      <p class="reason">{t('ai.fieldsBusy')}</p>
    {:else if blockedReason}
      <p class="reason">{blockedReason}</p>
    {/if}
  </div>

  <div class="foot">
    <label class="amend" class:disabled={!canAmend && !tab.amend} title={!canAmend ? (amendReason ?? undefined) : undefined}>
      <input
        type="checkbox"
        checked={tab.amend}
        disabled={(!canAmend && !tab.amend) || tab.committing}
        onchange={(e) => tab.setAmend(e.currentTarget.checked)}
      />
      <span class="box" aria-hidden="true"><Icon name="check" size={12} /></span>
      <span>{t('commit.amend')}</span>
    </label>
    {#if !canAmend && amendReason}
      <span class="sr-only">{amendReason}</span>
    {/if}
    <span class="spacer"></span>
    <span class="staged-count">{tn('commit.staged', tab.stagedCount)}</span>
    <button type="submit" class="btn btn-primary submit" data-testid="commit-submit" disabled={!canSubmit} aria-keyshortcuts="Control+Enter Meta+Enter">
      {#if tab.committing}
        <Icon name="loader" size={14} spin />{t('commit.creating')}
      {:else}
        <Icon name="git-commit" size={14} />{tab.amend ? t('commit.submitAmend') : t('commit.submit')}
      {/if}
      <kbd class="kbd">{submitCombo}</kbd>
    </button>
  </div>
</form>

{#if aiDialog}
  <AiConsentDialog {ai} onclose={() => (aiDialog = false)} />
{/if}

{#if ai.sheet && notice}
  <ProposalSheet
    kind="commit"
    title={notice.subject}
    body={notice.body}
    providerName={ai.providerName(notice.provider)}
    profileName={ai.state ? profileLabel(ai.state.profile.kind) : null}
    account={ai.accountLabel(notice.provider)}
    commandSummary={notice.commandSummary}
    stale={ai.stale}
    inUse={ai.inUse}
    regenerating={ai.regenerating}
    error={ai.sheetError}
    onuse={(v) => ai.useProposal(v)}
    onregenerate={() => void ai.regenerate()}
    ondiscard={() => ai.discard()}
    onclose={() => ai.closeSheet()}
  />
{/if}

<style>
  .commit {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    padding: var(--space-3);
    border-top: 1px solid var(--border);
    background: var(--bg-surface);
  }
  .head {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }
  .lbl {
    font-size: var(--text-xs);
    font-weight: 600;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--text-muted);
  }
  .desc-lbl {
    margin-top: var(--space-1);
  }
  .counter {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    font-size: var(--text-xs);
    font-variant-numeric: tabular-nums;
    color: var(--text-muted);
  }
  .counter.over {
    color: var(--danger);
    font-weight: 600;
  }
  .subject-row {
    display: flex;
    align-items: stretch;
    gap: var(--space-2);
  }
  .subject-row .field {
    flex: 1;
    min-width: 0;
  }
  .subject-row .field[aria-invalid='true'] {
    border-color: var(--danger);
  }
  .ai-btn,
  .ai-cancel,
  .ai-busy {
    flex: none;
    white-space: nowrap;
  }
  .ai-btn {
    color: var(--accent);
    border-color: color-mix(in srgb, var(--accent) 40%, transparent);
    background: var(--accent-soft);
  }
  .ai-btn.is-disabled {
    opacity: 0.55;
    cursor: not-allowed;
  }
  .ai-busy {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 0 var(--space-2);
    font-size: var(--text-sm);
    color: var(--accent);
  }
  textarea {
    min-height: 88px;
    padding: var(--space-2) var(--space-3);
    resize: vertical;
    line-height: 1.45;
  }
  .field[readonly] {
    opacity: 0.75;
  }
  .strip {
    display: flex;
    align-items: flex-start;
    gap: var(--space-2);
    padding: var(--space-2) var(--space-3);
    border-radius: var(--radius-md);
    font-size: var(--text-sm);
  }
  .strip .msg {
    display: flex;
    flex: 1;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
    align-self: center;
  }
  .strip.ok {
    align-items: center;
    background: var(--info-soft);
    color: var(--text);
  }
  .strip.ok .msg {
    flex-direction: row;
  }
  .strip.bad {
    background: var(--danger-soft);
  }
  .strip.bad > :global(svg) {
    color: var(--danger);
    margin-top: 3px;
  }
  .strip.info {
    background: var(--bg-subtle);
    border: 1px solid var(--border);
  }
  .cmd {
    font-family: var(--font-mono, monospace);
    font-size: var(--text-xs);
    color: var(--text-muted);
  }
  .hints {
    display: flex;
    flex-direction: column;
    gap: 2px;
    font-size: var(--text-xs);
  }
  .hints:empty {
    display: none;
  }
  .warn {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: var(--text-xs);
    color: var(--warning);
  }
  .reason {
    color: var(--text-subtle);
  }
  .foot {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    flex-wrap: wrap;
  }
  .spacer {
    flex: 1;
  }
  .staged-count {
    color: var(--text-subtle);
    font-size: var(--text-xs);
  }
  .amend {
    position: relative;
    display: inline-flex;
    align-items: center;
    gap: var(--space-2);
    min-height: var(--target-min);
    font-size: var(--text-sm);
    color: var(--text-muted);
    cursor: pointer;
  }
  .amend.disabled {
    opacity: 0.55;
    cursor: not-allowed;
  }
  .amend input {
    position: absolute;
    inset: 0 auto 0 0;
    width: 24px;
    margin: 0;
    opacity: 0;
    cursor: inherit;
  }
  .box {
    display: grid;
    place-items: center;
    width: 16px;
    height: 16px;
    margin: 0 0 0 4px;
    border: 1.5px solid var(--border-strong);
    border-radius: var(--radius-sm);
    background: var(--bg-surface);
    color: transparent;
  }
  .amend input:checked + .box {
    background: var(--accent);
    border-color: var(--accent);
    color: var(--accent-fg);
  }
  .amend input:focus-visible + .box {
    outline: 2px solid var(--focus-ring);
    outline-offset: 2px;
  }
  .submit {
    min-height: 34px;
    padding: 0 var(--space-3);
  }
  .kbd {
    margin-left: var(--space-1);
    background: color-mix(in srgb, var(--accent-fg) 18%, transparent);
    border-color: color-mix(in srgb, var(--accent-fg) 40%, transparent);
    color: var(--accent-fg);
  }
  .ai-cancel .kbd {
    background: var(--bg-subtle);
    border-color: var(--border-hover);
    color: var(--text-muted);
  }
  @media (forced-colors: active) {
    .amend input:checked + .box {
      background: Highlight;
      color: HighlightText;
    }
  }
</style>
