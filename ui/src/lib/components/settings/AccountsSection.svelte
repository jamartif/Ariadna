<script lang="ts">
  import { t } from '$lib/i18n';
  import type { MessageKey } from '$lib/i18n';
  import type { SettingsPanel } from '$lib/stores/settingsPanel.svelte';
  import { toasts } from '$lib/stores/toasts.svelte';
  import { patGuide } from '$lib/util/remote';
  import type { RemoteProvider } from '$lib/util/remote';
  import Icon from '../Icon.svelte';
  import ProviderMark from '../ProviderMark.svelte';

  let { panel }: { panel: SettingsPanel } = $props();

  const PROVIDERS: { id: Exclude<RemoteProvider, 'other'>; name: MessageKey; write: MessageKey }[] = [
    { id: 'azure', name: 'settings.accounts.azure', write: 'settings.accounts.write.azure' },
    { id: 'github', name: 'settings.accounts.github', write: 'settings.accounts.write.github' },
    { id: 'gitlab', name: 'settings.accounts.gitlab', write: 'settings.accounts.write.gitlab' },
    { id: 'bitbucket', name: 'settings.accounts.bitbucket', write: 'settings.accounts.write.bitbucket' }
  ];

  async function open(url: string) {
    try {
      await panel.api.openExternalUrl(url);
    } catch (e) {
      toasts.error(t('clone.openGuideFailed'), e);
    }
  }
</script>

<div class="sx">
  <div class="group">
    <p class="lead"><Icon name="shield-check" size={16} /><span>{t('settings.accounts.intro')}</span></p>
    <ul class="notes">
      <li>{t('settings.accounts.clone')}</li>
      <li>{t('settings.accounts.ai')}</li>
    </ul>
  </div>

  <div class="group">
    <h3>{t('settings.accounts.oauth')}</h3>
    <p class="hint">{t('settings.accounts.oauthHint')}</p>
    {#if panel.draft}
      <div class="oauth-ids">
        <label class="lbl" for="oauth-gh">{t('settings.accounts.oauthGithub')}</label>
        <input
          id="oauth-gh"
          class="field mono"
          type="text"
          value={panel.draft.oauthGitHubClientId ?? ''}
          oninput={(e) => panel.draft && (panel.draft.oauthGitHubClientId = e.currentTarget.value)}
          placeholder={t('settings.accounts.oauthPlaceholder')}
          autocomplete="off"
          spellcheck="false"
        />
        <label class="lbl" for="oauth-gl">{t('settings.accounts.oauthGitlab')}</label>
        <input
          id="oauth-gl"
          class="field mono"
          type="text"
          value={panel.draft.oauthGitLabClientId ?? ''}
          oninput={(e) => panel.draft && (panel.draft.oauthGitLabClientId = e.currentTarget.value)}
          placeholder={t('settings.accounts.oauthPlaceholder')}
          autocomplete="off"
          spellcheck="false"
        />
      </div>
    {/if}
  </div>

  <div class="group">
    <h3>{t('settings.accounts.help')}</h3>
    <p class="hint">{t('settings.accounts.minimum')}</p>
    <ul class="provs">
      {#each PROVIDERS as p (p.id)}
        {@const guide = patGuide(p.id)}
        <li class="card">
          <div class="top">
            <ProviderMark provider={p.id} size={24} />
            <strong>{t(p.name)}</strong>
            <span class="grow"></span>
            {#if guide}
              <button type="button" class="btn btn-sm" onclick={() => void open(guide.url)} aria-label={t('settings.accounts.open', { name: t(p.name) })}>
                {t('clone.howToPat')}<Icon name="external-link" size={12} />
              </button>
            {/if}
          </div>
          {#if guide}
            <dl>
              <dt>{t('settings.accounts.readPerms')}</dt>
              <dd>{t(guide.perms).replace(/^Permisos mínimos:\s*/, '')}</dd>
              <dt>{t('settings.accounts.writePerms')}</dt>
              <dd>{t(p.write)}</dd>
            </dl>
          {/if}
        </li>
      {/each}
    </ul>
  </div>
</div>

<style>
  .lead {
    display: flex;
    gap: var(--space-2);
    align-items: flex-start;
    font-size: var(--text-sm);
    color: var(--text);
  }
  .lead :global(svg) {
    flex: none;
    margin-top: 2px;
    color: var(--success);
  }
  .notes {
    margin: 0;
    padding-left: 1.2em;
    font-size: var(--text-sm);
    color: var(--text-muted);
  }
  .provs {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }
  .card {
    padding: var(--space-3);
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    background: var(--bg-surface);
  }
  .top {
    display: flex;
    align-items: center;
    gap: var(--space-2);
  }
  .grow {
    flex: 1;
  }
  dl {
    display: grid;
    grid-template-columns: max-content 1fr;
    gap: 2px var(--space-3);
    margin: var(--space-2) 0 0;
    font-size: var(--text-xs);
  }
  dt {
    color: var(--text-muted);
  }
  dd {
    margin: 0;
    color: var(--text);
  }
  .oauth-ids {
    display: flex;
    flex-direction: column;
    gap: 6px;
    margin-top: var(--space-2);
    max-width: 420px;
  }
  .oauth-ids .lbl {
    font-size: var(--text-xs);
    font-weight: 600;
    color: var(--text-muted);
  }
</style>
