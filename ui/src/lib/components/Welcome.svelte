<script lang="ts">
  import { t } from '$lib/i18n';
  import type { AppStore } from '$lib/stores/app.svelte';
  import type { ApiError, RecentRepository } from '$lib/api/types';
  import { relativeTime } from '$lib/util/time';
  import Banner from './Banner.svelte';
  import EmptyState from './EmptyState.svelte';
  import Icon from './Icon.svelte';
  import Logo from './Logo.svelte';

  let { app, entryId }: { app: AppStore; entryId: string } = $props();

  let manual = $state('');
  let openError = $state.raw<ApiError | null>(null);
  let busyPath = $state<string | null>(null);

  const blocked = $derived(app.gitProblem !== null);
  const favorites = $derived(app.recents.filter((r) => r.isFavorite));
  const others = $derived(
    app.recents.filter((r) => !r.isFavorite).sort((a, b) => b.lastOpened.localeCompare(a.lastOpened))
  );

  async function open(path: string) {
    openError = null;
    busyPath = path;
    const out = await app.openPath(path, entryId);
    busyPath = null;
    if (!out.ok) openError = out.error;
  }

  async function browse() {
    const path = await app.browse();
    if (path) await open(path);
  }

  function submitManual(e: SubmitEvent) {
    e.preventDefault();
    const p = manual.trim();
    if (p) void open(p);
  }
</script>

{#snippet recentRow(r: RecentRepository)}
  <li class="recent">
    <button
      type="button"
      class="open"
      onclick={() => open(r.path)}
      disabled={blocked || busyPath !== null}
      aria-label={t('welcome.openRecent', { name: r.name })}
    >
      <span class="repo-icon" aria-hidden="true">
        {#if busyPath === r.path}<Icon name="loader" size={16} spin />{:else}<Icon name="folder" size={16} />{/if}
      </span>
      <span class="text">
        <span class="name">{r.name}</span>
        <span class="path mono">{r.path}</span>
      </span>
      <span class="meta">
        {#if r.lastBranch}
          <span class="branch"><Icon name="git-branch" size={12} />{r.lastBranch}</span>
        {/if}
        <span class="when">{relativeTime(r.lastOpened)}</span>
      </span>
    </button>
    <button
      type="button"
      class="btn btn-ghost btn-icon star"
      class:on={r.isFavorite}
      aria-pressed={r.isFavorite}
      aria-label={r.isFavorite ? t('welcome.unfavorite', { name: r.name }) : t('welcome.favorite', { name: r.name })}
      title={r.isFavorite ? t('welcome.unfavorite', { name: r.name }) : t('welcome.favorite', { name: r.name })}
      onclick={() => app.toggleFavorite(r.path)}
    >
      <Icon name="star" size={16} filled={r.isFavorite} />
    </button>
    <button
      type="button"
      class="btn btn-ghost btn-icon forget"
      aria-label={t('welcome.forget', { name: r.name })}
      title={t('welcome.forget', { name: r.name })}
      onclick={() => app.forget(r.path)}
    >
      <Icon name="x" size={14} />
    </button>
  </li>
{/snippet}

<div class="welcome">
  <div class="column">
    <header class="hero">
      <Logo size={56} />
      <div>
        <h1>{t('welcome.title')}</h1>
        <p class="lead">{t('welcome.lead')}</p>
      </div>
    </header>

    {#if app.gitProblem}
      <Banner kind="error" title={t('welcome.gitProblemTitle')} message={app.gitProblem}>
        <ol class="steps">
          <li>{t('welcome.gitStep1')}</li>
          <li>{t('welcome.gitStep2')}</li>
          <li>{t('welcome.gitStep3')}</li>
        </ol>
        {#snippet actions()}
          <button type="button" class="btn" onclick={() => app.reload()}>
            <Icon name="refresh-cw" size={14} />{t('welcome.recheck')}
          </button>
        {/snippet}
      </Banner>
    {/if}

    <section class="card open-card" aria-labelledby="open-h">
      <h2 id="open-h" class="sr-only">{t('welcome.open')}</h2>
      <div class="row">
        <button type="button" class="btn btn-primary browse" onclick={browse} disabled={blocked || busyPath !== null}>
          <Icon name="folder-open" size={16} />{t('welcome.browse')}
        </button>
        <span class="or">{t('welcome.or')}</span>
        <form class="manual" onsubmit={submitManual}>
          <label class="sr-only" for="manual-path-{entryId}">{t('welcome.pathLabel')}</label>
          <input
            id="manual-path-{entryId}"
            class="field mono"
            type="text"
            bind:value={manual}
            placeholder={t('welcome.pathPlaceholder')}
            autocomplete="off"
            spellcheck="false"
            disabled={blocked}
            aria-invalid={openError ? 'true' : undefined}
            aria-describedby={openError ? 'open-error' : undefined}
          />
          <button type="submit" class="btn" disabled={blocked || manual.trim() === '' || busyPath !== null}>
            {t('welcome.openButton')}
          </button>
        </form>
      </div>
      {#if openError}
        <div id="open-error" class="open-error" role="alert">
          <Icon name="circle-alert" size={14} />
          <span>{openError.message}</span>
        </div>
      {/if}
    </section>

    {#if app.recents.length === 0}
      <section class="card" aria-label={t('welcome.recents')}>
        <EmptyState icon="git-branch" title={t('welcome.emptyTitle')} body={t('welcome.emptyBody')} compact>
          <button type="button" class="btn" onclick={browse} disabled={blocked}>
            <Icon name="folder-open" size={14} />{t('welcome.browse')}
          </button>
        </EmptyState>
      </section>
    {:else}
      {#if favorites.length > 0}
        <section aria-labelledby="fav-h">
          <h2 id="fav-h" class="section-title">
            <Icon name="star" size={13} filled />{t('welcome.favorites')}
          </h2>
          <ul class="list card">
            {#each favorites as r (r.path)}{@render recentRow(r)}{/each}
          </ul>
        </section>
      {/if}
      <section aria-labelledby="rec-h">
        <h2 id="rec-h" class="section-title"><Icon name="history" size={13} />{t('welcome.recents')}</h2>
        {#if others.length > 0}
          <ul class="list card">
            {#each others as r (r.path)}{@render recentRow(r)}{/each}
          </ul>
        {:else}
          <p class="muted">{t('welcome.onlyFavorites')}</p>
        {/if}
      </section>
    {/if}

    <footer class="tips">
      <span><kbd>Ctrl</kbd> <kbd>K</kbd> {t('welcome.tipPalette')}</span>
      <span><kbd>Ctrl</kbd> <kbd>T</kbd> {t('welcome.tipNewTab')}</span>
    </footer>
  </div>
</div>

<style>
  .welcome {
    height: 100%;
    overflow: auto;
    background:
      radial-gradient(900px 360px at 50% -80px, color-mix(in srgb, var(--accent-brand) 14%, transparent), transparent 70%),
      var(--bg-app);
  }
  .column {
    display: flex;
    flex-direction: column;
    gap: var(--space-5);
    width: min(760px, 100%);
    margin: 0 auto;
    padding: var(--space-7) var(--space-5) var(--space-6);
  }
  .hero {
    display: flex;
    align-items: center;
    gap: var(--space-4);
  }
  h1 {
    font-size: var(--text-2xl);
    font-weight: 700;
    letter-spacing: -0.02em;
    line-height: 1.15;
  }
  .lead {
    color: var(--text-muted);
    font-size: var(--text-lg);
  }
  .card {
    background: var(--bg-surface);
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    box-shadow: var(--elev-1);
  }
  .open-card {
    padding: var(--space-4);
  }
  .row {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    flex-wrap: wrap;
  }
  .browse {
    min-height: 36px;
    padding: 0 var(--space-4);
  }
  .or {
    color: var(--text-subtle);
    font-size: var(--text-sm);
  }
  .manual {
    display: flex;
    flex: 1;
    gap: var(--space-2);
    min-width: 260px;
  }
  .manual .field {
    min-height: 36px;
    font-size: var(--text-sm);
  }
  .manual .btn {
    min-height: 36px;
  }
  .open-error {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    margin-top: var(--space-3);
    color: var(--danger);
    font-size: var(--text-sm);
  }
  .section-title {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    margin-bottom: var(--space-2);
    color: var(--text-muted);
    font-size: var(--text-xs);
    font-weight: 600;
    letter-spacing: 0.06em;
    text-transform: uppercase;
  }
  .list {
    list-style: none;
    margin: 0;
    padding: var(--space-1);
  }
  .recent {
    display: flex;
    align-items: center;
    gap: 2px;
    border-radius: var(--radius-md);
  }
  .recent:hover {
    background: var(--bg-subtle);
  }
  .open {
    display: flex;
    flex: 1;
    align-items: center;
    gap: var(--space-3);
    min-width: 0;
    min-height: 48px;
    padding: var(--space-2) var(--space-3);
    border: none;
    background: none;
    text-align: left;
  }
  .open:disabled {
    opacity: 0.6;
  }
  .repo-icon {
    display: grid;
    place-items: center;
    width: 32px;
    height: 32px;
    flex: none;
    border-radius: var(--radius-md);
    background: var(--accent-soft);
    color: var(--accent);
  }
  .text {
    display: flex;
    flex-direction: column;
    min-width: 0;
    flex: 1;
  }
  .name {
    font-weight: 600;
  }
  .path {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--text-subtle);
    font-size: var(--text-xs);
  }
  .meta {
    display: flex;
    flex-direction: column;
    align-items: flex-end;
    gap: 2px;
    flex: none;
    color: var(--text-subtle);
    font-size: var(--text-xs);
  }
  .branch {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    font-family: var(--font-mono);
  }
  .star.on {
    color: var(--warning);
  }
  .forget {
    opacity: 0;
  }
  .recent:hover .forget,
  .forget:focus-visible {
    opacity: 1;
  }
  .muted {
    color: var(--text-subtle);
    font-size: var(--text-sm);
  }
  .steps {
    margin: var(--space-2) 0 0;
    padding-left: var(--space-5);
    color: var(--text);
    font-size: var(--text-sm);
  }
  .tips {
    display: flex;
    gap: var(--space-5);
    justify-content: center;
    color: var(--text-subtle);
    font-size: var(--text-xs);
  }
  @media (max-width: 640px) {
    .column {
      padding: var(--space-5) var(--space-3);
    }
    .meta {
      display: none;
    }
    .hero {
      flex-direction: column;
      align-items: flex-start;
    }
  }
</style>
