<script lang="ts">
  import { t } from '$lib/i18n';
  import type { MessageKey } from '$lib/i18n';
  import type { AiAccount, AiChoice, AiProfileKind, CliStatus } from '$lib/api/types';
  import type { RuleProblem, SettingsPanel } from '$lib/stores/settingsPanel.svelte';
  import { toasts } from '$lib/stores/toasts.svelte';
  import Icon from '../Icon.svelte';

  let { panel }: { panel: SettingsPanel } = $props();

  const ai = $derived(panel.ai);
  const rule = $derived(panel.effective?.profile.rule ? panel.effective.profile : null);
  const draft = $derived(panel.draft);
  const cliById = $derived(new Map((panel.cli ?? []).map((c) => [c.id, c])));
  const profileName = (k: AiProfileKind) => (k === 'empresa' ? 'Empresa' : 'Personal');

  let loginFor = $state<string | null>(null);
  let newPattern = $state('');
  let newProfile = $state<AiProfileKind>('empresa');
  let newProblem = $state<RuleProblem | null>(null);

  function cliText(c: CliStatus | undefined): string {
    if (!c) return t('settings.ai.checking');
    return c.availability === 'available' ? t('settings.ai.installed') : c.availability === 'failed' ? t('settings.ai.failed') : t('settings.ai.notFound');
  }

  function accountWho(a: AiAccount): string {
    const who = a.email ?? a.organization ?? '—';
    return a.organization && a.email ? `${who} · ${a.organization}` : who;
  }

  async function launch(id: string) {
    const info = await panel.openLogin(id);
    if (info?.launched) toasts.success(t('settings.ai.terminalOpened'));
    else if (!info) toasts.error(t('settings.ai.loginFailed'), panel.logins[id]?.error ?? undefined);
  }

  async function copy(cmd: string) {
    try {
      await navigator.clipboard.writeText(cmd);
      toasts.success(t('settings.ai.copied'));
    } catch {
      toasts.warning(t('settings.ai.copyFailed'));
    }
  }

  function addRule() {
    newProblem = panel.addRule(newPattern, newProfile);
    if (!newProblem) newPattern = '';
  }
</script>

{#if draft}
  <div class="sx">
    {#if !ai}
      <div class="loading" aria-busy="true"><span class="skeleton" style:width="100%" style:height="44px"></span></div>
    {:else}
      <label class="check">
        <input type="checkbox" checked={ai.enabled} disabled={panel.aiBusy} onchange={(e) => void panel.patchAi({ enabled: e.currentTarget.checked })} aria-describedby="set-ai-hint" />
        <span>
          <span class="main">{t('settings.ai.enable')}</span>
          <span class="hint" id="set-ai-hint">{t('settings.ai.enableHint')}</span>
        </span>
      </label>

      <fieldset class="group" disabled={!ai.enabled}>
        <legend>{t('settings.ai.profile')}</legend>
        <div class="seg" role="radiogroup" aria-label={t('settings.ai.profile')}>
          {#each ['personal', 'empresa'] as const as kind (kind)}
            <label class="opt" class:on={ai.profile.kind === kind}>
              <input type="radio" name="set-ai-profile" value={kind} checked={ai.profile.kind === kind} onchange={() => void panel.patchAi({ profile: kind })} />
              <span>{profileName(kind)}</span>
            </label>
          {/each}
        </div>
        <p class="hint">{t('settings.ai.profileHint')}</p>
        <label class="check iso">
          <input type="checkbox" checked={ai.profile.isolated} disabled={panel.aiBusy} onchange={(e) => void panel.patchAi({ isolated: e.currentTarget.checked })} aria-describedby="set-ai-iso-hint" />
          <span>
            <span class="main">{t('settings.ai.isolate')}</span>
            <span class="hint" id="set-ai-iso-hint">{t('settings.ai.isolateHint')}</span>
          </span>
        </label>
        {#if rule}
          <p class="status ok" role="status"><Icon name="info" size={14} /><span>{t('settings.ai.activeRule', { pattern: rule.rule ?? '', profile: rule.name })}</span></p>
        {:else if !panel.tabId}
          <p class="hint">{t('settings.ai.noRepo')}</p>
        {/if}
      </fieldset>

      <fieldset class="group" disabled={!ai.enabled}>
        <legend>{t('settings.ai.provider')}</legend>
        <div class="providers" role="radiogroup" aria-label={t('settings.ai.provider')}>
          <label class="prov" class:on={ai.choice === 'auto'}>
            <input type="radio" name="set-ai-provider" value="auto" checked={ai.choice === 'auto'} onchange={() => void panel.patchAi({ choice: 'auto' })} aria-labelledby="set-p-auto" aria-describedby="set-p-auto-h" />
            <span class="pname" id="set-p-auto">{t('settings.ai.providerAuto')}</span>
            <span class="pmeta" id="set-p-auto-h">{t('settings.ai.providerAutoHint')}</span>
          </label>
          {#each ai.providers as p (p.id)}
            {@const c = cliById.get(p.id)}
            {@const acc = panel.accountOf(p.id)}
            {@const login = panel.logins[p.id]}
            {@const loginErr = login?.error ?? null}
            {@const loginCmd = login?.info?.command ?? p.loginCommand}
            <div class="provwrap" class:off={!p.enabled}>
              <label class="prov" class:on={ai.choice === p.id}>
                <input type="radio" name="set-ai-provider" value={p.id} checked={ai.choice === p.id} onchange={() => void panel.patchAi({ choice: p.id as AiChoice })} aria-labelledby="set-p-{p.id}" aria-describedby="set-p-{p.id}-m" />
                <span class="pname" id="set-p-{p.id}">{p.displayName}</span>
                <span class="pmeta" id="set-p-{p.id}-m">
                  <span class="state" data-state={c?.availability ?? 'pending'}>{cliText(c)}</span>
                  {#if c?.version}· {t('settings.ai.version', { version: c.version })}{/if}
                  · {!p.enabled ? t('settings.ai.disabledByProfile', { profile: ai.profile.name }) : p.consented ? t('settings.ai.consented') : t('settings.ai.notConsented')}
                  {#if c?.path}<span class="path mono">{t('settings.ai.path', { path: c.path })}</span>{/if}
                  {#if c && !c.installed && c.hint}<span class="path">{c.hint}</span>{/if}
                </span>
              </label>
              <div class="sess" data-testid="session-{p.id}">
                {#if p.sharesLogin}
                  <span class="badge warn"><Icon name="triangle-alert" size={12} />{t('settings.ai.sharedBadge')}</span>
                {:else if p.isolated}
                  <span class="badge">{t('settings.ai.isolatedBadge')}</span>
                {:else}
                  <span class="badge">{t('settings.ai.normalSession')}</span>
                {/if}
                {#if acc}
                  {#if acc.state === 'loggedIn'}
                    <span class="acct">{t('settings.ai.accountIn', { who: accountWho(acc) })}</span>
                  {:else if acc.state === 'loggedOut'}
                    <span class="acct">{t('settings.ai.accountOut')}</span>
                  {:else}
                    <span class="acct muted" title={acc.reason ?? undefined}>{t('settings.ai.accountNA')}</span>
                  {/if}
                {:else if panel.accountsLoading}
                  <span class="acct muted">{t('settings.ai.accountLoading')}</span>
                {/if}
              </div>
              {#if p.sharesLogin}
                <p class="hint warnp" role="note">{t('settings.ai.sharedWarn', { name: p.displayName })}</p>
              {:else if p.isolated && !p.isolationVerified}
                <p class="hint">{t('settings.ai.unverified')}</p>
              {/if}
              <div class="prow">
                <label class="mini">
                  <input type="checkbox" checked={p.enabled} disabled={panel.aiBusy} onchange={(e) => void panel.patchAi({ providerEnabled: { id: p.id, enabled: e.currentTarget.checked } })} />
                  <span>{t('settings.ai.usable')}</span>
                </label>
                <button
                  type="button"
                  class="btn btn-sm"
                  aria-expanded={loginFor === p.id}
                  aria-controls="set-login-{p.id}"
                  onclick={() => {
                    loginFor = loginFor === p.id ? null : p.id;
                    if (loginFor === p.id) void panel.prepareLogin(p.id);
                  }}
                >
                  <Icon name="terminal" size={13} />{loginFor === p.id ? t('settings.ai.hideLogin') : t('settings.ai.login')}
                </button>
              </div>
              {#if loginFor === p.id}
                <div class="login" id="set-login-{p.id}" role="group" aria-label={t('settings.ai.loginTitle', { name: p.displayName })}>
                  <p>{t('settings.ai.loginBody')}</p>
                  {#if loginErr}<p class="hint bad" role="alert">{loginErr.message}</p>{/if}
                  <div class="cmdrow">
                    <code class="cmd mono">{loginCmd}</code>
                    <button type="button" class="btn btn-sm" onclick={() => void copy(loginCmd)} aria-label={`${t('settings.ai.copy')}: ${loginCmd}`}>
                      <Icon name="copy" size={13} />{t('settings.ai.copy')}
                    </button>
                    <button type="button" class="btn btn-sm btn-primary" disabled={!!loginErr} onclick={() => void launch(p.id)}>
                      <Icon name="terminal" size={13} />{t('settings.ai.openTerminal')}
                    </button>
                  </div>
                  {#if login && login.info?.command === null}<p class="hint warnp">{t('settings.ai.loginUnsafe')}</p>{/if}
                  {#if login?.info && !login.info.verified}<p class="hint">{t('settings.ai.loginUnverified')}</p>{/if}
                  {#if login?.info?.note}<p class="hint" role="status">{login.info.note}</p>{/if}
                </div>
              {/if}
            </div>
          {/each}
        </div>
        <div class="row">
          <button type="button" class="btn btn-sm btn-ghost" onclick={() => void panel.loadCli()} disabled={panel.cliLoading}>
            <Icon name="refresh-cw" size={13} spin={panel.cliLoading} />{panel.cliLoading ? t('settings.ai.checking') : t('settings.ai.refresh')}
          </button>
          <button type="button" class="btn btn-sm btn-ghost" onclick={() => void panel.loadAccounts()} disabled={panel.accountsLoading}>
            <Icon name="refresh-cw" size={13} spin={panel.accountsLoading} />{t('settings.ai.accountRefresh')}
          </button>
        </div>
      </fieldset>
    {/if}

    <div class="group">
      <h3 id="set-rules-h">{t('settings.ai.rules')}</h3>
      <p class="hint">{t('settings.ai.rulesHint')}</p>
      {#if draft.profileRules.length === 0}
        <p class="hint">{t('settings.ai.rulesEmpty')}</p>
      {:else}
        <ul class="rules" aria-labelledby="set-rules-h">
          {#each draft.profileRules as r, i (r.pattern)}
            <li>
              <span class="pat mono">{r.pattern}</span>
              <select class="field" aria-label={`${t('settings.ai.ruleProfile')}: ${r.pattern}`} value={r.profile} onchange={(e) => panel.setRuleProfile(i, e.currentTarget.value as AiProfileKind)}>
                <option value="personal">Personal</option>
                <option value="empresa">Empresa</option>
              </select>
              <button type="button" class="btn btn-ghost btn-icon" onclick={() => panel.removeRule(i)} aria-label={t('settings.ai.ruleRemove', { pattern: r.pattern })} title={t('settings.ai.ruleRemove', { pattern: r.pattern })}>
                <Icon name="trash" size={14} />
              </button>
            </li>
          {/each}
        </ul>
      {/if}
      <form
        class="add"
        onsubmit={(e) => {
          e.preventDefault();
          addRule();
        }}
      >
        <label class="sr-only" for="set-rule-new">{t('settings.ai.ruleNew')}</label>
        <input
          id="set-rule-new"
          class="field mono newpat"
          type="text"
          spellcheck="false"
          autocomplete="off"
          placeholder={t('settings.ai.ruleNewPlaceholder')}
          bind:value={newPattern}
          oninput={() => (newProblem = null)}
          aria-invalid={newProblem !== null}
          aria-describedby="set-rule-err"
        />
        <label class="sr-only" for="set-rule-profile">{t('settings.ai.ruleProfile')}</label>
        <select id="set-rule-profile" class="field prof" bind:value={newProfile}>
          <option value="personal">Personal</option>
          <option value="empresa">Empresa</option>
        </select>
        <button type="submit" class="btn"><Icon name="plus" size={14} />{t('settings.ai.ruleAdd')}</button>
      </form>
      <p class="hint bad" id="set-rule-err" style:color={newProblem ? 'var(--danger)' : undefined} role={newProblem ? 'alert' : undefined}>
        {newProblem ? t(`settings.ai.ruleProblem.${newProblem}` as MessageKey) : ''}
      </p>
    </div>
  </div>
{/if}

<style>
  .providers {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .provwrap {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .prov {
    position: relative;
    display: grid;
    grid-template-columns: 7em 1fr;
    align-items: baseline;
    gap: var(--space-3);
    padding: var(--space-2) var(--space-3);
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    cursor: pointer;
  }
  .prov:hover {
    border-color: var(--border-hover);
  }
  .prov.on {
    border-color: var(--accent);
    background: var(--accent-soft);
  }
  .prov input {
    position: absolute;
    inset: 0;
    opacity: 0;
    margin: 0;
    cursor: pointer;
  }
  .prov:has(input:focus-visible) {
    outline: 2px solid var(--focus-ring);
    outline-offset: 2px;
  }
  .provwrap.off .pname {
    color: var(--text-subtle);
  }
  .pname {
    font-weight: 600;
    font-size: var(--text-sm);
  }
  .pmeta {
    display: flex;
    flex-wrap: wrap;
    gap: 0 var(--space-1);
    font-size: var(--text-xs);
    color: var(--text-muted);
  }
  .path {
    flex-basis: 100%;
    color: var(--text-muted);
    word-break: break-all;
  }
  .state[data-state='available'] {
    color: var(--success);
  }
  .state[data-state='failed'] {
    color: var(--warning);
  }
  .prow {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-2);
    padding: 0 var(--space-1);
  }
  .mini {
    display: inline-flex;
    align-items: center;
    gap: var(--space-2);
    font-size: var(--text-xs);
    color: var(--text-muted);
    cursor: pointer;
  }
  .mini input {
    accent-color: var(--accent);
    width: 15px;
    height: 15px;
  }
  .mini input:focus-visible {
    outline: 2px solid var(--focus-ring);
    outline-offset: 2px;
  }
  .login {
    padding: var(--space-3);
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    background: var(--bg-subtle);
    font-size: var(--text-sm);
  }
  .login p {
    margin-bottom: var(--space-2);
    color: var(--text-muted);
  }
  .cmdrow {
    display: flex;
    align-items: center;
    gap: var(--space-2);
  }
  .cmd {
    flex: 1;
    min-width: 0;
    padding: 6px var(--space-3);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    background: var(--bg-surface);
    user-select: all;
  }
  .rules {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .rules li {
    display: flex;
    align-items: center;
    gap: var(--space-2);
  }
  .pat {
    flex: 1;
    min-width: 0;
    font-size: var(--text-sm);
  }
  .rules :global(.field),
  .prof {
    width: 130px;
    flex: none;
  }
  .add {
    display: flex;
    gap: var(--space-2);
    align-items: center;
  }
  .newpat {
    flex: 1;
    min-width: 0;
    width: auto;
  }
  .hint.bad {
    min-height: 1.2em;
    color: var(--danger);
  }
  .iso {
    margin-top: var(--space-2);
  }
  .sess {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-2);
    padding-left: 2px;
    font-size: var(--text-xs);
  }
  .badge {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 1px 8px;
    border: 1px solid var(--border);
    border-radius: 999px;
    background: var(--bg-subtle);
    color: var(--text-muted);
  }
  .badge.warn {
    border-color: color-mix(in srgb, var(--warning) 50%, transparent);
    color: var(--warning);
  }
  .acct {
    color: var(--text);
  }
  .acct.muted {
    color: var(--text-subtle);
  }
  .warnp {
    color: var(--warning);
  }
</style>
