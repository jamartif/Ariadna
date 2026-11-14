<script lang="ts">
  import { onDestroy, onMount, untrack } from 'svelte';
  import type { AppStore } from '$lib/stores/app.svelte';
  import { CloneFlow } from '$lib/stores/clone.svelte';
  import type { AuthMethodId, CliAuthStatus } from '$lib/api/types';
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
  // El selector de método solo tiene sentido si hay algo más que el PAT y la URL es HTTPS.
  const showMethods = $derived(flow.wantsCredentials && flow.methods.length > 1);
  const oauthPanel = $derived(flow.method === 'oauth' && flow.oauthState !== 'idle');

  const providerName = $derived(t(provider === 'other' ? 'clone.provider.generic' : `clone.provider.${provider}`));
  const hint = $derived.by(() => {
    if (!g) return '';
    const parts = [g.owner, g.project, g.repo].filter((x): x is string => !!x);
    return parts.join(' / ');
  });

  function cliState(id: 'gh' | 'glab'): CliAuthStatus | null {
    return flow.authOptions?.cli.find((c) => c.id === id) ?? null;
  }

  function cliReady(id: 'gh' | 'glab'): boolean {
    const c = cliState(id);
    return !!c?.installed && !!c?.loggedIn;
  }

  /** Texto corto bajo el nombre del método (el estado sale de Rust, nunca de un token). */
  function methodNote(m: AuthMethodId): string {
    if (m === 'pat') return t('clone.method.patNote');
    if (m === 'oauth') return t('clone.method.oauthNote');
    const c = cliState(m as 'gh' | 'glab');
    if (!c) return '';
    if (!c.installed) return t('clone.method.cliMissing', { tool: m });
    if (!c.loggedIn) return t('clone.method.cliLoggedOut', { label: c.label });
    return c.version ? t('clone.method.version', { version: c.version }) : t('clone.method.ready');
  }

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

  async function openVerification() {
    const url = flow.oauth?.verificationUri;
    if (!url) return;
    try {
      await app.api.openExternalUrl(url);
    } catch (e) {
      toasts.error(t('clone.openGuideFailed'), e);
    }
  }

  async function copyCode() {
    const code = flow.oauth?.userCode;
    if (!code) return;
    try {
      await navigator.clipboard.writeText(code);
      toasts.info(t('clone.method.codeCopied'));
    } catch {
      /* sin permiso de portapapeles: el código se ve en pantalla */
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
  onMount(() => {
    urlInput?.focus();
    void flow.loadAuthOptions();
  });
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
            onblur={() => flow.loadAuthOptions()}
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
              {#if showMethods}
                <div class="methods" role="radiogroup" aria-label={t('clone.method.label')}>
                  {#each flow.methods as m (m)}
                    <button
                      type="button"
                      class="method"
                      class:on={flow.method === m}
                      role="radio"
                      aria-checked={flow.method === m}
                      disabled={cloning}
                      onclick={() => flow.setMethod(m)}
                    >
                      <span class="dot" aria-hidden="true"></span>
                      <span class="m-body">
                        <span class="m-name">{t(`clone.method.${m}`)}</span>
                        <span class="m-sub">{methodNote(m)}</span>
                      </span>
                      {#if (m === 'gh' || m === 'glab') && flow.authOptions}
                        <span class="m-state" class:ok={cliReady(m as 'gh' | 'glab')}>
                          {cliReady(m as 'gh' | 'glab') ? t('clone.method.ready') : t('clone.method.notReady')}
                        </span>
                      {/if}
                    </button>
                  {/each}
                </div>
              {/if}

              {#if flow.method === 'pat'}
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
              {:else if flow.method === 'gh' || flow.method === 'glab'}
                {@const cli = cliState(flow.method)}
                <div class="cli">
                  <span class="cli-icon"><Icon name="terminal" size={14} /></span>
                  <div class="cli-body">
                    {#if cli?.installed && cli.loggedIn}
                      <p class="cli-title">{t('clone.method.cliReady', { label: cli.label })}</p>
                      {#if cli.version}<p class="cli-sub mono">{cli.version}</p>{/if}
                    {:else if cli?.installed}
                      <p class="cli-title">{t('clone.method.cliLoggedOut', { label: cli.label })}</p>
                      <p class="cli-sub">{t('clone.method.cliCommand', { command: cli.loginCommand })}</p>
                      <p class="cli-sub mono">{cli.loginCommand}</p>
                    {:else}
                      <p class="cli-title">{t('clone.method.cliMissing', { tool: flow.method })}</p>
                      <p class="cli-sub">{t('clone.method.cliCommand', { command: flow.method === 'gh' ? 'gh auth login' : 'glab auth login' })}</p>
                    {/if}
                  </div>
                </div>
              {:else if flow.method === 'oauth'}
                <div class="oauth">
                  {#if !flow.oauthAvailable}
                    <p class="hint"><Icon name="info" size={12} />{t('clone.method.oauthUnavailable')}</p>
                  {:else if flow.oauthState === 'idle'}
                    <button type="button" class="btn" onclick={() => flow.startOauth()} disabled={cloning}>
                      <Icon name="external-link" size={14} />{t('clone.method.oauthStart')}
                    </button>
                  {:else if flow.oauthState === 'starting'}
                    <p class="hint"><Icon name="loader" size={12} />{t('clone.method.oauthWaiting')}</p>
                  {:else if oauthPanel && flow.oauth}
                    <p class="lbl">{t('clone.method.oauthCode')}</p>
                    <div class="code-row">
                      <output class="code mono" aria-label={t('clone.method.oauthCode')}>{flow.oauth.userCode}</output>
                      <button type="button" class="btn btn-ghost btn-sm" onclick={copyCode} aria-label={t('clone.method.copyCode')}>
                        <Icon name="copy" size={14} />
                      </button>
                      <button type="button" class="btn btn-ghost btn-sm" onclick={openVerification}>
                        <Icon name="external-link" size={12} />{t('clone.method.oauthOpen')}
                      </button>
                    </div>
                    <p class="hint" class:good={flow.oauthState === 'authorized'} aria-live="polite">
                      {#if flow.oauthState === 'waiting'}
                        <Icon name="loader" size={12} />{t('clone.method.oauthWaiting')}
                      {:else if flow.oauthState === 'authorized'}
                        <Icon name="circle-check" size={12} />{t('clone.method.oauthAuthorized')}
                      {:else if flow.oauthState === 'expired'}
                        <Icon name="circle-alert" size={12} />{t('clone.method.oauthExpired')}
                      {:else if flow.oauthState === 'denied'}
                        <Icon name="ban" size={12} />{t('clone.method.oauthDenied')}
                      {/if}
                    </p>
                  {:else if flow.oauthState === 'error'}
                    <p class="hint bad"><Icon name="circle-alert" size={12} />{flow.oauthError?.message || t('clone.method.oauthError')}</p>
                  {/if}
                </div>
              {/if}

              <div class="check-row">
                <button type="button" class="btn btn-ghost btn-sm" onclick={() => flow.check()} disabled={!flow.canCheck}>
                  <Icon name={flow.checkState === 'checking' ? 'loader' : 'shield-check'} size={14} />
                  {flow.checkState === 'checking' ? t('clone.checking') : t('clone.check')}
                </button>
                <span class="check-msg" aria-live="polite">
                  {#if flow.checkState === 'ok'}
                    <span class="good"><Icon name="circle-check" size={12} />{t('clone.checkOk')}</span>
                  {:else if flow.checkState === 'failed'}
                    <span class="bad"><Icon name="circle-alert" size={12} />{flow.checkError?.message || t('clone.checkFailed')}</span>
                  {/if}
                </span>
              </div>
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
  .hint.good {
    color: var(--success);
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
  .methods {
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: 3px;
    border-radius: var(--radius-md);
    background: var(--bg-subtle);
    border: 1px solid var(--border);
  }
  .method {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    width: 100%;
    padding: 7px var(--space-2);
    border: none;
    border-radius: var(--radius-sm);
    background: transparent;
    text-align: left;
    color: var(--text-muted);
  }
  .method:hover {
    background: var(--bg-hover);
  }
  .method.on {
    background: var(--bg-raised);
    box-shadow:
      var(--elev-inset),
      inset 2px 0 0 var(--accent);
    color: var(--text);
  }
  .method .dot {
    flex: none;
    width: 12px;
    height: 12px;
    border-radius: 50%;
    border: 1.5px solid var(--border-strong, var(--border-hover));
  }
  .method.on .dot {
    border-color: var(--accent);
    background: radial-gradient(circle, var(--accent) 0 45%, transparent 46%);
  }
  .m-body {
    display: flex;
    flex-direction: column;
    min-width: 0;
    flex: 1;
  }
  .m-name {
    color: var(--text);
    font-size: var(--text-sm);
    font-weight: 600;
  }
  .m-sub {
    color: var(--text-subtle);
    font-size: var(--text-xs);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .m-state {
    flex: none;
    padding: 1px 7px;
    border-radius: var(--pill);
    background: var(--bg-muted);
    color: var(--text-subtle);
    font-size: var(--text-xs);
  }
  .m-state.ok {
    background: var(--success-soft);
    color: var(--success);
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
  .cli {
    display: flex;
    gap: var(--space-2);
    padding: var(--space-2) var(--space-3);
    border-radius: var(--radius-md);
    background: var(--bg-subtle);
    border: 1px solid var(--border);
  }
  .cli-icon {
    display: grid;
    place-items: center;
    width: 24px;
    height: 24px;
    flex: none;
    border-radius: 7px;
    background: var(--bg-muted);
    color: var(--text-muted);
  }
  .cli-body {
    min-width: 0;
  }
  .cli-title {
    font-size: var(--text-sm);
    color: var(--text);
  }
  .cli-sub {
    font-size: var(--text-xs);
    color: var(--text-subtle);
    overflow-wrap: anywhere;
  }
  .oauth {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }
  .code-row {
    display: flex;
    align-items: center;
    gap: var(--space-2);
  }
  .code {
    padding: 4px 10px;
    border-radius: var(--radius-sm);
    background: var(--bg-muted);
    color: var(--text);
    font-size: var(--text-md);
    letter-spacing: 0.08em;
    font-weight: 650;
    user-select: all;
  }
  .check-row {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    padding-top: 2px;
  }
  .check-msg {
    font-size: var(--text-xs);
    color: var(--text-subtle);
    display: flex;
    align-items: center;
    gap: 4px;
    min-width: 0;
  }
  .check-msg .good {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    color: var(--success);
  }
  .check-msg .bad {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    color: var(--danger);
    overflow-wrap: anywhere;
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
