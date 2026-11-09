<script lang="ts">
  import { t } from '$lib/i18n';
  import type { SettingsPanel } from '$lib/stores/settingsPanel.svelte';
  import Icon from '../Icon.svelte';

  let { panel }: { panel: SettingsPanel } = $props();

  const draft = $derived(panel.draft);
  const check = $derived(panel.gitCheck);
  const inUse = $derived(panel.view?.git ?? null);
</script>

{#if draft}
  <div class="sx">
    <div class="group">
      <label class="lbl" for="set-git">{t('settings.git.path')}</label>
      <div class="row">
        <input
          id="set-git"
          class="field mono"
          type="text"
          spellcheck="false"
          autocomplete="off"
          placeholder={t('settings.git.pathPlaceholder')}
          value={draft.gitPath ?? ''}
          oninput={(e) => panel.setGitPath(e.currentTarget.value)}
          onkeydown={(e) => {
            if (e.key === 'Enter') {
              e.preventDefault();
              void panel.checkGit();
            }
          }}
          aria-describedby="set-git-hint set-git-status"
        />
        <button type="button" class="btn" onclick={() => void panel.detectGit()} disabled={panel.gitChecking}>
          <Icon name="search" size={14} />{t('settings.git.detect')}
        </button>
        <button type="button" class="btn" onclick={() => void panel.checkGit()} disabled={panel.gitChecking}>
          <Icon name="check" size={14} />{t('settings.git.check')}
        </button>
      </div>
      <p class="hint" id="set-git-hint">{t('settings.git.pathHint')}</p>
      <div id="set-git-status" aria-live="polite">
        {#if panel.gitChecking}
          <p class="status"><Icon name="loader" size={14} spin />{t('settings.git.checking')}</p>
        {:else if check?.ok}
          <p class="status ok">
            <Icon name="circle-check" size={14} />
            <span>{t(check.source === 'auto' ? 'settings.git.okAuto' : 'settings.git.ok', { version: check.version ?? '', path: check.path ?? '' })}</span>
          </p>
        {:else if check}
          <p class="status bad"><Icon name="circle-alert" size={14} /><span>{check.problem}</span></p>
        {:else if inUse}
          <p class="hint">{t('settings.git.inUse', { version: inUse.version, path: inUse.path })}</p>
        {:else}
          <p class="status bad"><Icon name="circle-alert" size={14} /><span>{panel.view?.gitProblem ?? t('settings.git.none')}</span></p>
        {/if}
      </div>
      {#if draft.gitPath !== panel.view?.settings.gitPath}
        <p class="hint">{t('settings.git.applyNote')}</p>
      {/if}
    </div>

    <label class="check">
      <input type="checkbox" checked={draft.pullRebase} onchange={(e) => (draft.pullRebase = e.currentTarget.checked)} aria-describedby="set-rebase-hint" />
      <span>
        <span class="main">{t('settings.git.rebase')}</span>
        <span class="hint" id="set-rebase-hint">{t('settings.git.rebaseHint')}</span>
      </span>
    </label>

    <div class="group">
      <h3>{t('settings.git.identity')}</h3>
      <p class="hint">{t('settings.git.identityHint')}</p>
      <div class="ident">
        <label class="lbl" for="set-id-name">{t('settings.git.name')}</label>
        <input id="set-id-name" class="field" type="text" readonly value={panel.identity?.name ?? ''} placeholder={t('settings.git.notSet')} />
        <label class="lbl" for="set-id-email">{t('settings.git.email')}</label>
        <input id="set-id-email" class="field" type="text" readonly value={panel.identity?.email ?? ''} placeholder={t('settings.git.notSet')} />
      </div>
    </div>
  </div>
{/if}

<style>
  .ident {
    display: grid;
    grid-template-columns: 6em 1fr;
    align-items: center;
    gap: var(--space-2) var(--space-3);
  }
  .ident .lbl {
    text-transform: none;
    letter-spacing: 0;
    font-size: var(--text-sm);
    font-weight: 500;
  }
  .ident :global(.field[readonly]) {
    background: var(--bg-subtle);
  }
  .check .hint {
    display: block;
    font-weight: 400;
  }
</style>
