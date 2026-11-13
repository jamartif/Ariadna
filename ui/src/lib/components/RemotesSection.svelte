<script lang="ts">
  import { t } from '$lib/i18n';
  import type { RemoteView } from '$lib/api/types';
  import type { TabState } from '$lib/stores/tab.svelte';
  import { markOf, remotesOf } from '$lib/stores/remotes.svelte';
  import { branchNameProblem } from '$lib/util/branchName';
  import Icon from './Icon.svelte';
  import ProviderMark from './ProviderMark.svelte';

  /** Remotos de la pestaña: lista con proveedor, traer/bajar/subir, añadir/renombrar/quitar y abrir en el navegador. */
  let { tab }: { tab: TabState } = $props();

  const r = $derived(remotesOf(tab));
  /** Cambia cuando se añade, quita o renombra un remoto, o cambia su URL. */
  const signature = $derived((tab.snapshot?.remotes ?? []).map((x) => `${x.name}\u0000${x.fetchUrl}`).join('\n'));

  $effect(() => {
    void signature;
    void r.load();
  });

  let open = $state(true);
  let adding = $state(false);
  let newName = $state('origin');
  let newUrl = $state('');
  let renaming = $state<string | null>(null);
  let renameTo = $state('');
  let nameInput = $state<HTMLInputElement>();
  let renameInput = $state<HTMLInputElement>();

  const addProblem = $derived(newName === '' ? null : branchNameProblem(newName));
  const renameProblem = $derived(renameTo === '' ? null : renameTo === renaming ? 'branches.problem.same' : branchNameProblem(renameTo));
  const anyBusy = $derived(r.isBusy());

  function startAdd() {
    adding = true;
    renaming = null;
    newName = (tab.snapshot?.remotes.length ?? 0) === 0 ? 'origin' : '';
    newUrl = '';
    queueMicrotask(() => nameInput?.focus());
  }

  async function submitAdd(e: Event) {
    e.preventDefault();
    if (newName.trim() === '' || addProblem || newUrl.trim() === '') return;
    if (await r.add(newName, newUrl)) adding = false;
  }

  function startRename(v: RemoteView) {
    renaming = v.name;
    renameTo = v.name;
    adding = false;
    queueMicrotask(() => renameInput?.select());
  }

  async function submitRename(e: Event) {
    e.preventDefault();
    if (!renaming || renameTo === '' || renameProblem) return;
    if (await r.rename(renaming, renameTo)) renaming = null;
  }

  function where(v: RemoteView): string {
    return v.host !== '' ? v.host : t('remotes.provider.local');
  }
</script>

<section class="remotes" aria-label={t('remotes.region')}>
  <h3>
    <button type="button" class="fold" aria-expanded={open} onclick={() => (open = !open)}>
      <Icon name={open ? 'chevron-down' : 'chevron-right'} size={12} />
      <span>{t('remotes.title')}</span>
      <span class="chip">{r.views.length}</span>
    </button>
    <button type="button" class="btn btn-ghost btn-icon add" onclick={startAdd} disabled={adding || anyBusy} aria-label={t('remotes.add')} title={t('remotes.add')}>
      <Icon name="plus" size={14} />
    </button>
  </h3>

  {#if open}
    {#if adding}
      <form class="form" onsubmit={submitAdd} aria-label={t('remotes.add')}>
        <label class="lbl" for="remote-new-name">{t('remotes.name')}</label>
        <input id="remote-new-name" bind:this={nameInput} class="field mono" bind:value={newName} placeholder={t('remotes.namePlaceholder')} autocomplete="off" spellcheck="false" aria-invalid={addProblem ? 'true' : undefined} />
        <label class="lbl" for="remote-new-url">{t('remotes.url')}</label>
        <input id="remote-new-url" class="field mono" bind:value={newUrl} placeholder="https://github.com/usuario/repo.git" autocomplete="off" spellcheck="false" aria-describedby="remote-new-hint" />
        <p id="remote-new-hint" class="hint" class:bad={addProblem !== null} aria-live="polite">
          {#if addProblem}<Icon name="circle-alert" size={12} />{t(addProblem)}{:else}{t('remotes.urlHint')}{/if}
        </p>
        <div class="actions">
          <button type="button" class="btn btn-ghost" onclick={() => (adding = false)}>{t('remotes.cancel')}</button>
          <button type="submit" class="btn btn-primary" disabled={newName.trim() === '' || addProblem !== null || newUrl.trim() === '' || anyBusy}>
            {#if r.isBusy('add', newName.trim())}<Icon name="loader" size={14} spin />{/if}{t('remotes.addSubmit')}
          </button>
        </div>
      </form>
    {/if}

    {#if r.views.length === 0 && r.loaded && !adding}
      <p class="none">{t('remotes.empty')}</p>
    {/if}

    <ul>
      {#each r.views as v (v.name)}
        <li class="remote" data-remote={v.name}>
          <div class="top">
            <ProviderMark provider={markOf(v.provider)} size={22} />
            <div class="titles">
              <span class="rname mono">{v.name}</span>
              <span class="prov">{v.providerLabel} · {where(v)}{v.ssh ? ` · ${t('remotes.ssh')}` : ''}</span>
            </div>
          </div>
          <span class="url mono" title={v.fetchUrl}>{v.fetchUrl}</span>

          {#if renaming === v.name}
            <form class="form inline" onsubmit={submitRename}>
              <label class="lbl" for="remote-rename-{v.name}">{t('remotes.renameTo')}</label>
              <input id="remote-rename-{v.name}" bind:this={renameInput} class="field mono" bind:value={renameTo} autocomplete="off" spellcheck="false" aria-invalid={renameProblem ? 'true' : undefined} />
              <p class="hint" class:bad={renameProblem !== null} aria-live="polite">{#if renameProblem}<Icon name="circle-alert" size={12} />{t(renameProblem)}{/if}</p>
              <div class="actions">
                <button type="button" class="btn btn-ghost" onclick={() => (renaming = null)}>{t('remotes.cancel')}</button>
                <button type="submit" class="btn btn-primary" disabled={renameTo === '' || renameProblem !== null || anyBusy}>{t('remotes.renameSubmit')}</button>
              </div>
            </form>
          {:else}
            <div class="tools" role="group" aria-label={v.name}>
              <button type="button" class="btn btn-ghost btn-icon" disabled={anyBusy} onclick={() => r.fetch(v.name)} aria-label={t('remotes.fetch')} title={t('remotes.fetch')}>
                <Icon name={r.isBusy('fetch', v.name) ? 'loader' : 'refresh-cw'} size={14} spin={r.isBusy('fetch', v.name)} />
              </button>
              <button type="button" class="btn btn-ghost btn-icon" disabled={anyBusy || !r.hasCurrent} onclick={() => r.pull(v.name)} aria-label={t('remotes.pull')} title={r.hasCurrent ? t('remotes.pull') : t('remotes.noCurrentBranch')}>
                <Icon name={r.isBusy('pull', v.name) ? 'loader' : 'arrow-down'} size={14} spin={r.isBusy('pull', v.name)} />
              </button>
              <button type="button" class="btn btn-ghost btn-icon" disabled={anyBusy || !r.hasCurrent} onclick={() => r.push(v.name)} aria-label={t('remotes.push')} title={r.hasCurrent ? t('remotes.push') : t('remotes.noCurrentBranch')}>
                <Icon name={r.isBusy('push', v.name) ? 'loader' : 'arrow-up'} size={14} spin={r.isBusy('push', v.name)} />
              </button>
              <button type="button" class="btn btn-ghost btn-icon" disabled={anyBusy || !v.canOpen} onclick={() => r.open(v.name, 'repo')} aria-label={t('remotes.openRepo')} title={v.canOpen ? t('remotes.openRepo') : t('remotes.noWeb')}>
                <Icon name="external-link" size={14} />
              </button>
              <span class="sp"></span>
              <button type="button" class="btn btn-ghost btn-icon" disabled={anyBusy} onclick={() => startRename(v)} aria-label={t('remotes.rename')} title={t('remotes.rename')}>
                <Icon name="pencil" size={14} />
              </button>
              <button type="button" class="btn btn-ghost btn-icon danger" disabled={anyBusy} onclick={() => r.remove(v.name)} aria-label={t('remotes.remove')} title={t('remotes.remove')}>
                <Icon name="trash" size={14} />
              </button>
            </div>
          {/if}
        </li>
      {/each}
    </ul>
  {/if}
</section>

<style>
  .remotes {
    border-bottom: 1px solid var(--border);
    padding-bottom: var(--space-2);
  }
  h3 {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin: var(--space-2) var(--space-3) var(--space-1);
    font-size: var(--text-xs);
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: var(--text-subtle);
  }
  .fold {
    display: inline-flex;
    align-items: center;
    gap: var(--space-2);
    border: none;
    background: none;
    padding: 0;
    color: inherit;
    font: inherit;
    text-transform: inherit;
    letter-spacing: inherit;
  }
  ul {
    list-style: none;
    margin: 0;
    padding: 0 var(--space-2);
    display: grid;
    gap: var(--space-1);
  }
  .remote {
    display: grid;
    gap: 2px;
    padding: var(--space-2);
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    background: var(--bg-subtle);
  }
  .top {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    min-width: 0;
  }
  .titles {
    display: grid;
    min-width: 0;
  }
  .rname {
    font-weight: 650;
    font-size: var(--text-sm);
  }
  .prov {
    font-size: var(--text-xs);
    color: var(--text-muted);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .url {
    font-size: var(--text-xs);
    color: var(--text-subtle);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .tools {
    display: flex;
    align-items: center;
    gap: 2px;
    margin-top: 2px;
  }
  .sp {
    flex: 1;
  }
  .danger {
    color: var(--danger);
  }
  .form {
    display: grid;
    gap: var(--space-1);
    margin: var(--space-1) var(--space-2) var(--space-2);
    padding: var(--space-3);
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    background: var(--bg-surface);
  }
  .form.inline {
    margin: var(--space-1) 0 0;
  }
  .lbl {
    font-size: var(--text-xs);
    font-weight: 600;
    color: var(--text-muted);
  }
  .hint {
    display: flex;
    align-items: center;
    gap: 6px;
    margin: var(--space-1) 0 0;
    font-size: var(--text-xs);
    color: var(--text-subtle);
  }
  .hint.bad {
    color: var(--danger);
  }
  .actions {
    display: flex;
    justify-content: flex-end;
    gap: var(--space-2);
    margin-top: var(--space-2);
  }
  .none {
    margin: var(--space-2) var(--space-3);
    font-size: var(--text-sm);
    color: var(--text-muted);
  }
</style>
