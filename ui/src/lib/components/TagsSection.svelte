<script lang="ts">
  import { intlLocale, t } from '$lib/i18n';
  import type { TabState } from '$lib/stores/tab.svelte';
  import { tagsOf } from '$lib/stores/tags.svelte';
  import { relativeTime } from '$lib/util/time';
  import Icon from './Icon.svelte';
  import TagCreateForm from './TagCreateForm.svelte';

  /** Etiquetas de la pestaña: lista (ligeras y anotadas), crear en HEAD, publicar y borrar. */
  let { tab }: { tab: TabState } = $props();

  const tags = $derived(tagsOf(tab));
  /** Cambia con cada mutación del repo (crear, borrar, checkout…): recarga la lista. */
  const revision = $derived(tab.revision);

  $effect(() => {
    void revision;
    void tags.load();
  });

  /** Por encima de este número se muestran solo las primeras hasta que se pida el resto. */
  const SHOWN = 50;
  let open = $state(true);
  let creating = $state(false);
  let all = $state(false);
  let remote = $state<string | null>(null);

  const remotes = $derived(tags.remoteNames);
  const chosen = $derived(remote && remotes.includes(remote) ? remote : tags.defaultRemote);
  const shown = $derived(all ? tags.tags : tags.tags.slice(0, SHOWN));
  const anyBusy = $derived(tags.isBusy());
</script>

<section class="tags" aria-label={t('tags.region')}>
  <h3>
    <button type="button" class="fold" aria-expanded={open} onclick={() => (open = !open)}>
      <Icon name={open ? 'chevron-down' : 'chevron-right'} size={12} />
      <span>{t('tags.title')}</span>
      <span class="chip">{tags.tags.length}</span>
    </button>
    <button type="button" class="btn btn-ghost btn-icon add" onclick={() => (creating = true)} disabled={creating || anyBusy} aria-label={t('tags.new')} title={t('tags.new')}>
      <Icon name="plus" size={14} />
    </button>
  </h3>

  {#if open}
    {#if creating}
      <TagCreateForm {tab} idPrefix="tag-list" onclose={() => (creating = false)} />
    {/if}

    {#if remotes.length > 1}
      <div class="remote-pick">
        <label for="tag-remote">{t('tags.remote')}</label>
        <select id="tag-remote" class="field" value={chosen} onchange={(e) => (remote = e.currentTarget.value)}>
          {#each remotes as r (r)}<option value={r}>{r}</option>{/each}
        </select>
      </div>
    {/if}

    {#if tags.tags.length === 0 && tags.loaded && !creating}
      <p class="none">{t('tags.empty')}</p>
    {/if}

    <ul>
      {#each shown as tag (tag.name)}
        <li class="tag" data-tag={tag.name}>
          <div class="top">
            <Icon name="tag" size={14} />
            <span class="tname mono" title={tag.name}>{tag.name}</span>
            <span class="kind" class:annotated={tag.annotated}>{tag.annotated ? t('tags.annotated') : t('tags.lightweight')}</span>
          </div>
          {#if tag.message}<p class="msg">{tag.message}</p>{/if}
          <p class="meta">
            <code class="mono" title={tag.commit}>{tag.commit.slice(0, 7)}</code>
            <span aria-hidden="true">·</span>
            <time datetime={tag.dateIso} title={new Date(tag.dateIso).toLocaleString(intlLocale())}>{relativeTime(tag.dateIso)}</time>
            {#if tag.tagger}<span aria-hidden="true">·</span><span>{t('tags.by', { name: tag.tagger })}</span>{/if}
          </p>
          <div class="tools" role="group" aria-label={tag.name}>
            <button
              type="button"
              class="btn btn-ghost btn-icon"
              disabled={anyBusy || !chosen}
              onclick={() => chosen && tags.publish(chosen, tag.name)}
              aria-label={t('tags.publish', { name: tag.name })}
              title={chosen ? t('tags.publish', { name: tag.name }) : t('tags.noRemote')}
            >
              <Icon name={tags.isBusy('publish', tag.name) ? 'loader' : 'arrow-up'} size={14} spin={tags.isBusy('publish', tag.name)} />
            </button>
            <button
              type="button"
              class="btn btn-ghost btn-icon danger"
              disabled={anyBusy || !chosen}
              onclick={() => chosen && tags.removeRemote(chosen, tag.name)}
              aria-label={t('tags.deleteRemote', { name: tag.name })}
              title={chosen ? t('tags.deleteRemote', { name: tag.name }) : t('tags.noRemote')}
            >
              <Icon name={tags.isBusy('delete-remote', tag.name) ? 'loader' : 'cloud'} size={14} spin={tags.isBusy('delete-remote', tag.name)} />
            </button>
            <span class="sp"></span>
            <button
              type="button"
              class="btn btn-ghost btn-icon danger"
              disabled={anyBusy}
              onclick={() => tags.remove(tag.name)}
              aria-label={t('tags.delete', { name: tag.name })}
              title={t('tags.delete', { name: tag.name })}
            >
              <Icon name={tags.isBusy('delete', tag.name) ? 'loader' : 'trash'} size={14} spin={tags.isBusy('delete', tag.name)} />
            </button>
          </div>
        </li>
      {/each}
    </ul>
    {#if !all && tags.tags.length > SHOWN}
      <button type="button" class="btn btn-ghost more" onclick={() => (all = true)}>{t('tags.showAll', { n: tags.tags.length })}</button>
    {/if}
  {/if}
</section>

<style>
  .tags {
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
  .tag {
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
    color: var(--warning);
  }
  .tname {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-weight: 650;
    font-size: var(--text-sm);
    color: var(--text);
  }
  .kind {
    flex: none;
    padding: 0 6px;
    height: 18px;
    line-height: 18px;
    border-radius: var(--pill);
    font-size: var(--text-2xs);
    font-weight: 650;
    background: var(--bg-muted);
    color: var(--text-muted);
  }
  .kind.annotated {
    background: var(--warning-soft);
    color: var(--warning);
  }
  .msg {
    margin: 0;
    font-size: var(--text-xs);
    color: var(--text-muted);
    white-space: pre-line;
    display: -webkit-box;
    -webkit-line-clamp: 3;
    line-clamp: 3;
    -webkit-box-orient: vertical;
    overflow: hidden;
    overflow-wrap: anywhere;
  }
  .meta {
    display: flex;
    align-items: center;
    gap: var(--space-1);
    flex-wrap: wrap;
    margin: 0;
    font-size: var(--text-xs);
    color: var(--text-subtle);
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
  .remote-pick {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    margin: 0 var(--space-3) var(--space-1);
    font-size: var(--text-xs);
    color: var(--text-muted);
  }
  .remote-pick .field {
    flex: 1;
    min-width: 0;
  }
  .none {
    margin: var(--space-2) var(--space-3);
    font-size: var(--text-sm);
    color: var(--text-muted);
  }
  .more {
    margin: var(--space-1) var(--space-3) 0;
  }
</style>
