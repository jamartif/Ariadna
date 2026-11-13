<script lang="ts">
  import { onDestroy, onMount, untrack } from 'svelte';
  import type { AppStore } from '$lib/stores/app.svelte';
  import { CloneFlow } from '$lib/stores/clone.svelte';
  import { t } from '$lib/i18n';
  import { toasts } from '$lib/stores/toasts.svelte';
  import { focusTrap } from '$lib/util/focusTrap';
  import { patGuide, type RemoteProvider } from '$lib/util/remote';
  import Banner from './Banner.svelte';
  import Icon from './Icon.svelte';
  import ProviderMark from './ProviderMark.svelte';

  let { app, onclose }: { app: AppStore; onclose: () => void } = $props();

  // El diálogo se monta una sola vez por apertura: se toma la pestaña «nueva» activa en ese momento.
  const draftId = untrack(() => (app.activeEntry?.kind === 'new' ? app.activeEntry.id : undefined));
  const flow = new CloneFlow(untrack(() => app.api), (res) => {
    app.adopt(res, draftId);
    onclose();
  });
  onDestroy(() => flow.dispose());

  const g = $derived(flow.guess);
  const provider = $derived<RemoteProvider>(g?.provider ?? 'other');
  const guide = $derived(patGuide(provider));
  const cloning = $derived(flow.status === 'cloning');
  const urlTouched = $derived(flow.url.trim() !== '');
  const showProblem = $derived(urlTouched && flow.problem !== null);
  const percent = $derived(flow.progress?.percent ?? null);

  const providerName = $derived(t(provider === 'other' ? 'clone.provider.generic' : `clone.provider.${provider}`));
  const hint = $derived.by(() => {
    if (!g) return '';
    const parts = [g.owner, g.project, g.repo].filter((x): x is string => !!x);
    return parts.join(' / ');
  });

  async function pickFolder() {
    const p = await app.browse();
    if (p) flow.setParent(p);
  }

  async function openGuide() {
    if (!guide) return;
    try {
      await app.api.openExternalUrl(guide.url);
    } catch (e) {
      toasts.error(t('clone.openGuideFailed'), e);
    }
  }

  function key(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      e.preventDefault();
      e.stopPropagation();
      if (cloning) void flow.cancel();
      else onclose();
    }
  }

  let urlInput = $state<HTMLInputElement>();
  onMount(() => urlInput?.focus());
</script>

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="backdrop" onclick={() => !cloning && onclose()}>
  <div
    class="dialog float"
    role="dialog"
    aria-modal="true"
    aria-labelledby="clone-title"
    tabindex="-1"
    use:focusTrap
    onclick={(e) => e.stopPropagation()}
    onkeydown={key}
  >
    <header class="head">
      <span class="badge"><Icon name="download" size={18} /></span>
      <div class="titles">
        <h2 id="clone-title">{t('clone.title')}</h2>
        <p>{t('clone.subtitle')}</p>
      </div>
      <button type="button" class="btn btn-ghost btn-icon" onclick={() => (cloning ? flow.cancel() : onclose())} aria-label={t('clone.close')}>
        <Icon name="x" size={16} />
      </button>
    </header>

    <form
      class="body"
      onsubmit={(e) => {
        e.preventDefault();
        void flow.clone();
      }}
      autocomplete="off"
    >
      <div class="row">
        <label class="lbl" for="clone-url">{t('clone.url')}</label>
        <div class="url-wrap" class:invalid={showProblem}>
          <span class="url-mark">
            {#if g}<ProviderMark {provider} size={22} />{:else}<Icon name="link" size={16} />{/if}
          </span>
          <input
            id="clone-url"
            bind:this={urlInput}
            class="field mono"
            type="text"
            value={flow.url}
            oninput={(e) => flow.setUrl(e.currentTarget.value)}
            placeholder="https://dev.azure.com/org/proyecto/_git/repo"
            autocomplete="off"
            autocapitalize="off"
            spellcheck="false"
            disabled={cloning}
            aria-invalid={showProblem ? 'true' : undefined}
            aria-describedby="clone-url-hint"
          />
        </div>
        <p id="clone-url-hint" class="hint" class:bad={showProblem} aria-live="polite">
          {#if showProblem}
            <Icon name="circle-alert" size={12} />{t(`clone.problem.${flow.problem as 'empty'}`)}
          {:else if g}
            <span class="prov">{providerName}</span>
            <span class="mono path">{hint}</span>
            <span class="scheme">{g.scheme === 'ssh' ? t('clone.viaSsh') : t('clone.viaHttps')}</span>
          {:else}
            {t('clone.urlHelp')}
          {/if}
        </p>
      </div>

      <div class="row">
        <label class="lbl" for="clone-dest">{t('clone.destination')}</label>
        <div class="inline">
          <input
            id="clone-dest"
            class="field mono"
            type="text"
            value={flow.destination}
            oninput={(e) => flow.setDestination(e.currentTarget.value)}
            placeholder={t('clone.destinationPlaceholder')}
            autocomplete="off"
            spellcheck="false"
            disabled={cloning}
          />
          <button type="button" class="btn" onclick={pickFolder} disabled={cloning}>
            <Icon name="folder-open" size={14} />{t('clone.browse')}
          </button>
        </div>
      </div>

      <div class="row narrow">
        <label class="lbl" for="clone-branch">{t('clone.branch')} <span class="opt">{t('clone.optional')}</span></label>
        <input
          id="clone-branch"
          class="field mono"
          type="text"
          bind:value={flow.branch}
          placeholder={t('clone.branchPlaceholder')}
          autocomplete="off"
          spellcheck="false"
          disabled={cloning}
        />
      </div>

      {#if flow.wantsCredentials}
        <section class="cred" class:open={flow.credOpen} aria-label={t('clone.credentials')}>
          <button type="button" class="cred-head" aria-expanded={flow.credOpen} aria-controls="clone-cred-body" onclick={() => (flow.credOpen = !flow.credOpen)}>
            <span class="lock"><Icon name="lock" size={14} /></span>
            <span class="cred-title">{t('clone.credentials')}</span>
            <span class="cred-sub">{t('clone.credentialsHint')}</span>
            <Icon name={flow.credOpen ? 'chevron-up' : 'chevron-down'} size={14} />
          </button>
          {#if flow.credOpen}
            <div id="clone-cred-body" class="cred-body">
              <div class="two">
                <div class="row">
                  <label class="lbl" for="clone-user">{t('clone.username')}</label>
                  <input
                    id="clone-user"
                    class="field"
                    type="text"
                    bind:value={flow.username}
                    placeholder={provider === 'azure' ? t('clone.usernameAzure') : t('clone.usernamePlaceholder')}
                    autocomplete="off"
                    autocapitalize="off"
                    spellcheck="false"
                  />
                </div>
                <div class="row">
                  <label class="lbl" for="clone-pat">{t('clone.token')}</label>
                  <div class="pat-wrap">
                    <input
                      id="clone-pat"
                      class="field mono"
                      type={flow.showSecret ? 'text' : 'password'}
                      bind:value={flow.secret}
                      placeholder="••••••••••••••••"
                      autocomplete="off"
                      autocapitalize="off"
                      spellcheck="false"
                      data-1p-ignore
                      data-lpignore="true"
                    />
                    <button
                      type="button"
                      class="eye btn btn-ghost btn-icon btn-sm"
                      aria-label={flow.showSecret ? t('clone.hideToken') : t('clone.showToken')}
                      aria-pressed={flow.showSecret}
                      onclick={() => (flow.showSecret = !flow.showSecret)}
                    >
                      <Icon name={flow.showSecret ? 'eye-off' : 'eye'} size={14} />
                    </button>
                  </div>
                </div>
              </div>
              {#if provider === 'azure'}<p class="hint"><Icon name="info" size={12} />{t('clone.azureUserHint')}</p>{/if}
              <p class="safe"><Icon name="shield-check" size={14} /><span>{t('clone.tokenStorage')}</span></p>

              {#if guide}
                <div class="cred-actions">
                  <button type="button" class="btn btn-ghost btn-sm link" onclick={openGuide}>
                    {t('clone.howToPat')}<Icon name="external-link" size={12} />
                  </button>
                </div>
              {/if}
              {#if guide}<p class="hint perms"><Icon name="key" size={12} />{t(guide.perms)}</p>{/if}
            </div>
          {/if}
        </section>
      {:else if g && g.scheme === 'ssh'}
        <p class="ssh-note"><Icon name="key" size={14} />{t('clone.sshNote')}</p>
      {/if}

      {#if flow.error}
        <Banner kind="error" title={flow.error.code === 'AUTH' ? t('clone.authTitle') : t('clone.errorTitle')} message={flow.error.message} detail={flow.error.detail}>
          {#snippet actions()}
            {#if flow.error?.code === 'AUTH' && flow.wantsCredentials}
              <button type="button" class="btn" onclick={() => flow.openCredentials()}>
                <Icon name="key" size={14} />{t('clone.enterCredentials')}
              </button>
            {/if}
          {/snippet}
        </Banner>
      {/if}

      {#if cloning}
        <div class="progress" role="status" aria-live="polite">
          <div class="p-top">
            <span class="phase">{flow.progress?.phase || t('clone.starting')}</span>
            {#if percent !== null}<span class="pct num">{Math.round(percent)} %</span>{/if}
          </div>
          <div
            class="bar"
            class:indeterminate={percent === null}
            role="progressbar"
            aria-label={t('clone.progress')}
            aria-valuemin="0"
            aria-valuemax="100"
            aria-valuenow={percent === null ? undefined : Math.round(percent)}
          >
            <span class="fill" style:width={percent === null ? undefined : `${percent}%`}></span>
          </div>
        </div>
      {/if}

      <footer class="foot">
        {#if cloning}
          <button type="button" class="btn" onclick={() => flow.cancel()}>{t('clone.cancelClone')}</button>
        {:else}
          <button type="button" class="btn btn-ghost" onclick={onclose}>{t('clone.cancel')}</button>
          <button type="submit" class="btn btn-primary" disabled={!flow.canClone}>
            <Icon name="download" size={14} />{t('clone.submit')}
          </button>
        {/if}
      </footer>
    </form>
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
    padding: 9vh var(--space-4) var(--space-4);
    background: var(--bg-overlay);
    backdrop-filter: blur(3px);
    animation: fade-in var(--dur) var(--ease);
    overflow: auto;
  }
  .dialog {
    width: min(580px, 100%);
    border-radius: 16px;
    overflow: hidden;
  }
  .head {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    padding: var(--space-4) var(--space-4) var(--space-3);
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
  .body {
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
    padding: var(--space-2) var(--space-4) var(--space-4);
  }
  .row {
    display: flex;
    flex-direction: column;
    gap: 6px;
    min-width: 0;
  }
  .row.narrow {
    max-width: 240px;
  }
  .lbl {
    font-size: var(--text-xs);
    font-weight: 600;
    color: var(--text-muted);
    letter-spacing: 0.01em;
  }
  .opt {
    margin-left: 4px;
    font-weight: 400;
    color: var(--text-subtle);
  }
  .url-wrap {
    position: relative;
    display: flex;
    align-items: center;
  }
  .url-mark {
    position: absolute;
    left: 8px;
    display: grid;
    place-items: center;
    color: var(--text-subtle);
    pointer-events: none;
  }
  .url-wrap .field {
    padding-left: 38px;
    min-height: 36px;
    font-size: var(--text-sm);
  }
  .url-wrap.invalid .field {
    border-color: var(--danger);
  }
  .field.mono {
    font-size: var(--text-sm);
  }
  .inline {
    display: flex;
    gap: var(--space-2);
  }
  .inline .field,
  .inline .btn {
    min-height: 36px;
  }
  .hint {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 6px;
    min-height: 18px;
    color: var(--text-subtle);
    font-size: var(--text-xs);
  }
  .hint.bad {
    color: var(--danger);
  }
  .prov {
    padding: 1px 8px;
    border-radius: var(--pill);
    background: var(--accent-soft);
    color: var(--accent);
    font-weight: 600;
  }
  .path {
    color: var(--text-muted);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    max-width: 280px;
  }
  .scheme {
    margin-left: auto;
  }
  .cred {
    border: 1px solid var(--border-hover);
    border-radius: var(--radius-lg);
    background: var(--bg-raised);
    box-shadow: var(--elev-inset);
    overflow: hidden;
  }
  .cred.open {
    border-color: color-mix(in srgb, var(--accent) 38%, var(--border-hover));
  }
  .cred-head {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    width: 100%;
    min-height: 42px;
    padding: 0 var(--space-3);
    border: none;
    background: transparent;
    text-align: left;
    color: var(--text-muted);
  }
  .cred-head:hover {
    background: var(--bg-hover);
  }
  .lock {
    display: grid;
    place-items: center;
    width: 24px;
    height: 24px;
    border-radius: 7px;
    background: var(--bg-muted);
    color: var(--text-muted);
  }
  .cred-title {
    color: var(--text);
    font-weight: 600;
    font-size: var(--text-sm);
  }
  .cred-sub {
    flex: 1;
    font-size: var(--text-xs);
    color: var(--text-subtle);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .cred-body {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
    padding: var(--space-1) var(--space-3) var(--space-3);
    animation: rise-in var(--dur-slow) var(--ease-out);
  }
  .two {
    display: grid;
    grid-template-columns: 1fr 1.4fr;
    gap: var(--space-3);
  }
  .pat-wrap {
    position: relative;
    display: flex;
    align-items: center;
  }
  .pat-wrap .field {
    padding-right: 34px;
  }
  .eye {
    position: absolute;
    right: 3px;
  }
  .safe {
    display: flex;
    align-items: flex-start;
    gap: var(--space-2);
    padding: var(--space-2) var(--space-3);
    border-radius: var(--radius-md);
    background: var(--success-soft);
    color: var(--text-muted);
    font-size: var(--text-xs);
    line-height: 1.45;
  }
  .safe :global(svg) {
    color: var(--success);
    margin-top: 1px;
  }
  .cred-actions {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: var(--space-2);
  }
  .link {
    margin-left: auto;
    color: var(--accent);
  }
  .perms {
    margin-top: -4px;
  }
  .ssh-note {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    padding: var(--space-2) var(--space-3);
    border-radius: var(--radius-md);
    background: var(--bg-subtle);
    color: var(--text-muted);
    font-size: var(--text-xs);
  }
  .progress {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: var(--space-3);
    border-radius: var(--radius-md);
    background: var(--bg-subtle);
    border: 1px solid var(--border);
  }
  .p-top {
    display: flex;
    justify-content: space-between;
    font-size: var(--text-sm);
    color: var(--text-muted);
  }
  .pct {
    color: var(--text);
    font-weight: 600;
  }
  .bar {
    height: 6px;
    border-radius: var(--pill);
    background: var(--bg-muted);
    overflow: hidden;
  }
  .fill {
    display: block;
    height: 100%;
    border-radius: var(--pill);
    background: var(--grad-accent);
    box-shadow: var(--glow);
    transition: width var(--dur-slow) var(--ease);
  }
  .bar.indeterminate .fill {
    width: 38%;
    animation: slide 1.3s var(--ease) infinite;
  }
  @keyframes slide {
    from {
      transform: translateX(-100%);
    }
    to {
      transform: translateX(270%);
    }
  }
  .foot {
    display: flex;
    justify-content: flex-end;
    gap: var(--space-2);
    padding-top: var(--space-1);
  }
  .foot .btn {
    min-height: 34px;
  }
  @media (max-width: 520px) {
    .two {
      grid-template-columns: 1fr;
    }
  }
</style>
