<script lang="ts">
  import { t } from '$lib/i18n';
  import type { AiPrConflictOptions, PrConflictOption, PrConflicts } from '$lib/api/prTypes';
  import type { TabState } from '$lib/stores/tab.svelte';
  import { toasts } from '$lib/stores/toasts.svelte';
  import { focusTrap } from '$lib/util/focusTrap';
  import Icon from './Icon.svelte';

  /**
   * Conflictos de una pull request resueltos con IA, en tres pasos explícitos:
   *
   * 1. Se leen los ficheros en conflicto (solo lectura: `git merge-tree`).
   * 2. La IA propone alternativas por fichero; se ven completas antes de elegir.
   * 3. «Aplicar y preparar merge» deja la PR en local con las resoluciones escritas y
   *    preparadas; el commit y el push los hace el usuario. Nada se escribe antes de pulsar.
   */
  let { tab, prNumber, onclose }: { tab: TabState; prNumber: number; onclose: () => void } =
    $props();

  let conflicts = $state<PrConflicts | null>(null);
  let loading = $state(true);
  let error = $state<string | null>(null);
  let applying = $state(false);
  let done = $state<string | null>(null);
  /** Fichero que se está proponiendo con IA (para el indicador). */
  let proposing = $state<string | null>(null);
  /** Alternativas por ruta y la elegida. */
  let proposals = $state<Record<string, PrConflictOption[]>>({});
  let chosen = $state<Record<string, number>>({});

  void load();

  async function load(): Promise<void> {
    loading = true;
    error = null;
    try {
      conflicts = await tab.api.prConflicts(tab.id, prNumber);
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    } finally {
      loading = false;
    }
  }

  async function propose(path: string): Promise<void> {
    proposing = path;
    try {
      const res: AiPrConflictOptions = await tab.api.aiPrConflictOptions(tab.id, prNumber, path);
      proposals = { ...proposals, [path]: res.options };
      chosen = { ...chosen, [path]: 0 };
    } catch (e) {
      toasts.error(e instanceof Error ? e.message : String(e), e);
    } finally {
      proposing = null;
    }
  }

  async function proposeAll(): Promise<void> {
    for (const f of conflicts?.files ?? []) {
      if (f.binary || f.truncated) continue;
      await propose(f.path);
    }
  }

  const pendingCount = $derived(
    (conflicts?.files ?? []).filter(
      (f) => !f.binary && !f.truncated && chosen[f.path] === undefined
    ).length
  );

  async function apply(): Promise<void> {
    if (!conflicts || applying) return;
    const resolutions = conflicts.files
      .filter((f) => chosen[f.path] !== undefined)
      .map((f) => ({
        path: f.path,
        content: (proposals[f.path] ?? [])[chosen[f.path]!]!.content
      }));
    if (resolutions.length === 0) return;
    applying = true;
    try {
      const res = await tab.api.prApplyConflicts(tab.id, prNumber, resolutions);
      if (!res.done) {
        toasts.error(t('pr.conflicts.declined'), null);
        return;
      }
      done = res.branch;
      void tab.refresh();
    } catch (e) {
      toasts.error(e instanceof Error ? e.message : String(e), e);
    } finally {
      applying = false;
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
    aria-labelledby="pr-conflicts-title"
    tabindex="-1"
    data-testid="pr-conflicts-dialog"
    use:focusTrap
    onclick={(e) => e.stopPropagation()}
  >
    <header class="head">
      <span class="badge"><Icon name="triangle-alert" size={16} /></span>
      <h2 id="pr-conflicts-title">{t('pr.conflicts.title', { number: prNumber })}</h2>
      <button type="button" class="btn btn-ghost btn-icon" aria-label={t('pr.cancel')} onclick={onclose}>
        <Icon name="x" size={14} />
      </button>
    </header>

    {#if loading}
      <p class="state" role="status" data-testid="pr-conflicts-loading">
        <Icon name="loader" size={14} spin />{t('pr.conflicts.loading')}
      </p>
    {:else if error}
      <div class="state bad" role="alert">
        <p>{error}</p>
        <button type="button" class="btn btn-sm" onclick={() => void load()}>{t('common.retry')}</button>
      </div>
    {:else if conflicts}
      {#if done}
        <p class="ok" role="status" data-testid="pr-conflicts-done">
          <Icon name="circle-check" size={14} />
          {t('pr.conflicts.applied', { branch: done })}
        </p>
      {:else}
        <p class="hint">{t('pr.conflicts.intro', { base: conflicts.base, head: conflicts.head })}</p>
      {/if}

      {#if conflicts.files.length === 0}
        <p class="hint" data-testid="pr-conflicts-empty">{t('pr.conflicts.none')}</p>
      {:else}
        <ul class="files">
          {#each conflicts.files as f (f.path)}
            <li class="file" data-testid="pr-conflicts-file">
              <div class="frow">
                <span class="path mono">{f.path}</span>
                {#if f.binary}
                  <span class="chip">{t('pr.conflicts.binary')}</span>
                {:else if f.truncated}
                  <span class="chip warn">{t('pr.conflicts.tooBig')}</span>
                {:else if chosen[f.path] !== undefined}
                  <span class="chip ok">{t('pr.conflicts.chosen')}</span>
                {/if}
                <button
                  type="button"
                  class="btn btn-sm"
                  disabled={f.binary || f.truncated || proposing !== null || applying}
                  onclick={() => void propose(f.path)}
                  data-testid="pr-conflicts-propose"
                >
                  {#if proposing === f.path}
                    <Icon name="loader" size={13} spin />{t('pr.conflicts.proposing')}
                  {:else}
                    <Icon name="sparkles" size={13} />{t('pr.conflicts.propose')}
                  {/if}
                </button>
              </div>
              {#if proposals[f.path]}
                <fieldset class="options">
                  <legend>{t('pr.conflicts.optionsLegend')}</legend>
                  {#each proposals[f.path] ?? [] as o, i (i)}
                    <label class="opt">
                      <input
                        type="radio"
                        name="opt-{f.path}"
                        checked={chosen[f.path] === i}
                        onchange={() => (chosen = { ...chosen, [f.path]: i })}
                        data-testid="pr-conflicts-option"
                      />
                      <span class="otitle">{o.title || t('pr.conflicts.optionN', { n: i + 1 })}</span>
                      {#if o.rationale}<span class="orat">{o.rationale}</span>{/if}
                    </label>
                    {#if chosen[f.path] === i}
                      <pre class="preview" data-testid="pr-conflicts-preview">{o.content}</pre>
                    {/if}
                  {/each}
                </fieldset>
              {/if}
            </li>
          {/each}
        </ul>
      {/if}

      {#if conflicts.files.length > 0 && !done}
        <div class="actions">
          <button
            type="button"
            class="btn btn-sm"
            disabled={proposing !== null || applying}
            onclick={() => void proposeAll()}
          >
            <Icon name="sparkles" size={13} />{t('pr.conflicts.proposeAll')}
          </button>
          <span class="grow"></span>
          <button
            type="button"
            class="btn btn-sm primary"
            disabled={applying || pendingCount > 0}
            onclick={() => void apply()}
            data-testid="pr-conflicts-apply"
          >
            <Icon name="git-merge" size={13} />{applying ? t('pr.conflicts.applying') : t('pr.conflicts.apply')}
          </button>
        </div>
        <p class="hint">{t('pr.conflicts.applyHint')}</p>
      {/if}
    {/if}
  </div>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 95;
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
    width: min(760px, 100%);
    max-height: min(86vh, 780px);
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
  .ok {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    margin: 0;
    font-size: var(--text-sm);
    color: var(--accent);
  }
  .files {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    gap: var(--space-3);
  }
  .file {
    display: grid;
    gap: var(--space-2);
    padding: var(--space-2);
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    background: var(--bg-subtle);
  }
  .frow {
    display: flex;
    align-items: center;
    gap: var(--space-2);
  }
  .path {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: var(--text-sm);
  }
  .options {
    display: grid;
    gap: var(--space-2);
    margin: 0;
    padding: 0;
    border: 0;
  }
  .options legend {
    font-size: var(--text-xs);
    color: var(--text-muted);
    padding: 0;
  }
  .opt {
    display: flex;
    align-items: baseline;
    gap: var(--space-2);
    cursor: pointer;
    font-size: var(--text-sm);
  }
  .otitle {
    font-weight: 600;
  }
  .orat {
    color: var(--text-muted);
    font-size: var(--text-xs);
  }
  .preview {
    margin: 0;
    max-height: 220px;
    overflow: auto;
    padding: var(--space-2);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    background: var(--bg-app);
    font-size: var(--text-xs);
    white-space: pre-wrap;
  }
  .actions {
    display: flex;
    align-items: center;
    gap: var(--space-2);
  }
  .grow {
    flex: 1;
  }
  .hint {
    margin: 0;
    font-size: var(--text-xs);
    color: var(--text-muted);
  }
</style>