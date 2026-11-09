<script lang="ts">
  import { onMount } from 'svelte';
  import { t, tn } from '$lib/i18n';
  import { formatCombo, isMac, registerHandlers, SHORTCUTS } from '$lib/shortcuts/registry';
  import type { TabState } from '$lib/stores/tab.svelte';
  import Icon from './Icon.svelte';

  let { tab }: { tab: TabState } = $props();

  const SUBJECT_LIMIT = 72;

  let textarea = $state<HTMLTextAreaElement>();
  let canAmend = $state(true);
  let amendReason = $state<string | null>(null);

  const submitCombo = formatCombo(SHORTCUTS.find((s) => s.id === 'commit.submit')?.combo ?? 'Mod+Enter', isMac());

  const subject = $derived(tab.message.split('\n')[0] ?? '');
  const subjectLen = $derived([...subject].length);
  const overLimit = $derived(subjectLen > SUBJECT_LIMIT);
  const secondLineFilled = $derived((tab.message.split('\n')[1] ?? '').trim() !== '');
  const empty = $derived(tab.message.trim() === '');
  const nothingStaged = $derived(tab.stagedCount === 0 && !tab.amend);
  const blockedReason = $derived(
    tab.committing ? null : empty ? t('commit.reasonEmpty') : nothingStaged ? t('commit.reasonNothing') : null
  );
  const canSubmit = $derived(!tab.committing && !empty && !nothingStaged);
  const detached = $derived(tab.snapshot?.status.isDetached === true);

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

  async function submit() {
    if (!canSubmit) return;
    await tab.submit();
  }

  onMount(() =>
    registerHandlers({
      'commit.submit': () => {
        if (!canSubmit) return false;
        void submit();
        return true;
      },
      'commit.focus': () => {
        textarea?.focus();
        return true;
      }
    })
  );
</script>

<form
  class="commit"
  aria-label={t('commit.region')}
  onsubmit={(e) => {
    e.preventDefault();
    void submit();
  }}
>
  <div class="head">
    <label for="commit-msg-{tab.id}" class="lbl">{t('commit.message')}</label>
    <span class="counter" class:over={overLimit} id="subject-count-{tab.id}">
      {#if overLimit}<Icon name="triangle-alert" size={12} />{/if}
      {t('commit.subjectCount', { n: subjectLen, max: SUBJECT_LIMIT })}
    </span>
  </div>

  <textarea
    id="commit-msg-{tab.id}"
    bind:this={textarea}
    class="field"
    rows="4"
    data-scope="editor"
    placeholder={tab.amend ? t('commit.placeholderAmend') : t('commit.placeholder')}
    value={tab.message}
    oninput={(e) => tab.setMessage(e.currentTarget.value)}
    aria-describedby="subject-count-{tab.id} commit-hint-{tab.id}"
    aria-invalid={overLimit ? 'true' : undefined}
    spellcheck="true"
    disabled={tab.committing}
  ></textarea>

  <div id="commit-hint-{tab.id}" class="hints">
    {#if overLimit}
      <p class="warn"><Icon name="triangle-alert" size={12} />{t('commit.subjectTooLong', { max: SUBJECT_LIMIT })}</p>
    {/if}
    {#if secondLineFilled}
      <p class="warn"><Icon name="info" size={12} />{t('commit.secondLine')}</p>
    {/if}
    {#if detached && !tab.amend}
      <p class="warn"><Icon name="triangle-alert" size={12} />{t('commit.detached')}</p>
    {/if}
    {#if blockedReason}
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
    <button type="submit" class="btn btn-primary submit" disabled={!canSubmit} aria-keyshortcuts="Control+Enter Meta+Enter">
      {#if tab.committing}
        <Icon name="loader" size={14} spin />{t('commit.creating')}
      {:else}
        <Icon name="git-commit" size={14} />{tab.amend ? t('commit.submitAmend') : t('commit.submit')}
      {/if}
      <kbd class="kbd">{submitCombo}</kbd>
    </button>
  </div>
</form>

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
  textarea {
    min-height: 88px;
    padding: var(--space-2) var(--space-3);
    resize: vertical;
    line-height: 1.45;
  }
  textarea[aria-invalid='true'] {
    border-color: var(--danger);
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
  @media (forced-colors: active) {
    .amend input:checked + .box {
      background: Highlight;
      color: HighlightText;
    }
  }
</style>
