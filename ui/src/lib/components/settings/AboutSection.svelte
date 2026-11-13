<script lang="ts">
  import { t } from '$lib/i18n';
  import type { SettingsPanel } from '$lib/stores/settingsPanel.svelte';
  import { toasts } from '$lib/stores/toasts.svelte';
  import Icon from '../Icon.svelte';
  import Logo from '../Logo.svelte';

  let { panel }: { panel: SettingsPanel } = $props();

  const about = $derived(panel.view?.about);
  const git = $derived(panel.view?.git);

  async function open(url: string) {
    try {
      await panel.api.openExternalUrl(url);
    } catch (e) {
      toasts.error(t('clone.openGuideFailed'), e);
    }
  }
</script>

{#if about}
  <div class="sx">
    <div class="brand">
      <Logo size={44} />
      <div>
        <h3>Ariadna</h3>
        <p class="hint">{t('settings.about.tagline')}</p>
      </div>
    </div>

    <dl>
      <dt>{t('settings.about.version')}</dt>
      <dd>{about.version}</dd>
      <dt>{t('settings.about.license')}</dt>
      <dd>{about.license}</dd>
      <dt>{t('settings.about.git')}</dt>
      <dd>{git ? `${git.version} (${git.path})` : t('settings.about.gitNone')}</dd>
    </dl>

    <div class="group">
      <h3>{t('settings.about.links')}</h3>
      <div class="row links">
        <button type="button" class="btn" onclick={() => void open(about.repository)}>
          <Icon name="git-fork" size={14} />{t('settings.about.repo')}<Icon name="external-link" size={12} />
        </button>
        <button type="button" class="btn" onclick={() => void open(about.issues)}>
          <Icon name="circle-alert" size={14} />{t('settings.about.issues')}<Icon name="external-link" size={12} />
        </button>
      </div>
    </div>
  </div>
{/if}

<style>
  .brand {
    display: flex;
    align-items: center;
    gap: var(--space-3);
  }
  dl {
    display: grid;
    grid-template-columns: max-content 1fr;
    gap: var(--space-2) var(--space-4);
    margin: 0;
    font-size: var(--text-sm);
  }
  dt {
    color: var(--text-muted);
  }
  dd {
    margin: 0;
    word-break: break-all;
  }
  .links {
    flex-wrap: wrap;
  }
</style>
