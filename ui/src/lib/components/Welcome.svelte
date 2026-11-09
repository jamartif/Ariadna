<script lang="ts">
  import { t, tn } from '$lib/i18n';
  import { formatCombo, isMac, SHORTCUTS } from '$lib/shortcuts/registry';
  import type { AppStore } from '$lib/stores/app.svelte';
  import type { ApiError, RecentRepository } from '$lib/api/types';
  import { repoHue } from '$lib/util/hue';
  import { relativeTime } from '$lib/util/time';
  import Banner from './Banner.svelte';
  import Icon from './Icon.svelte';
  import Logo from './Logo.svelte';

  let { app, entryId }: { app: AppStore; entryId: string } = $props();

  let manual = $state('');
  let openError = $state.raw<ApiError | null>(null);
  let busyPath = $state<string | null>(null);

  const blocked = $derived(app.gitProblem !== null);
  const favorites = $derived(app.recents.filter((r) => r.isFavorite));
  const others = $derived(app.recents.filter((r) => !r.isFavorite).sort((a, b) => b.lastOpened.localeCompare(a.lastOpened)));
  const mac = isMac();
  const cloneCombo = formatCombo(SHORTCUTS.find((s) => s.id === 'clone.open')?.combo ?? 'Mod+Shift+O', mac);
  const paletteCombo = formatCombo(SHORTCUTS.find((s) => s.id === 'palette.open')?.combo ?? 'Mod+K', mac);

  /** Nº de cambios conocido solo si el repo está abierto en una pestaña. */
  function changesOf(path: string): number | null {
    return app.repoTabs.find((x) => x.path === path)?.changeCount ?? null;
  }

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

{#snippet repoCard(r: RecentRepository)}
  {@const changes = changesOf(r.path)}
  <li class="repo" style:--hue={repoHue(r.name)}>
    <button
      type="button"
      class="open"
      onclick={() => open(r.path)}
      disabled={blocked || busyPath !== null}
      aria-label={t('welcome.openRecent', { name: r.name })}
    >
      <span class="top">
        <span class="dot" aria-hidden="true">
          {#if busyPath === r.path}<Icon name="loader" size={12} spin />{/if}
        </span>
        <span class="name">{r.name}</span>
      </span>
      <span class="path mono" title={r.path}>{r.path}</span>
      <span class="meta">
        {#if r.lastBranch}
          <span class="branch mono"><Icon name="git-branch" size={11} />{r.lastBranch}</span>
        {/if}
        {#if changes !== null && changes > 0}
          <span class="chg num">{tn('welcome.changes', changes)}</span>
        {/if}
        <span class="when">{relativeTime(r.lastOpened)}</span>
      </span>
    </button>
    <button
      type="button"
      class="icon star"
      class:on={r.isFavorite}
      aria-pressed={r.isFavorite}
      aria-label={r.isFavorite ? t('welcome.unfavorite', { name: r.name }) : t('welcome.favorite', { name: r.name })}
      title={r.isFavorite ? t('welcome.unfavorite', { name: r.name }) : t('welcome.favorite', { name: r.name })}
      onclick={() => app.toggleFavorite(r.path)}
    >
      <Icon name="star" size={15} filled={r.isFavorite} />
    </button>
    <button
      type="button"
      class="icon forget"
      aria-label={t('welcome.forget', { name: r.name })}
      title={t('welcome.forget', { name: r.name })}
      onclick={() => app.forget(r.path)}
    >
      <Icon name="x" size={13} />
    </button>
  </li>
{/snippet}

<div class="welcome">
  <div class="bg" aria-hidden="true">
    <svg class="thread" viewBox="0 0 400 400" fill="none" focusable="false">
      <defs>
        <linearGradient id="thread-g" x1="0" y1="0" x2="1" y2="1">
          <stop offset="0" stop-color="var(--accent-brand)" stop-opacity="0.9" />
          <stop offset="1" stop-color="var(--accent-brand)" stop-opacity="0" />
        </linearGradient>
      </defs>
      <path
        d="M200 360c-88 0-160-72-160-160S112 40 200 40s128 58 128 128-52 104-104 104-80-40-80-80 28-56 56-56"
        stroke="url(#thread-g)"
        stroke-width="3"
        stroke-linecap="round"
      />
      <circle cx="200" cy="136" r="7" fill="var(--accent-brand)" />
    </svg>
  </div>

  <div class="column">
    <header class="hero">
      <Logo size={44} />
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

    <section class="actions" aria-labelledby="open-h">
      <h2 id="open-h" class="sr-only">{t('welcome.open')}</h2>

      <button type="button" class="action primary" onclick={browse} disabled={blocked || busyPath !== null}>
        <span class="a-icon"><Icon name="folder-open" size={20} /></span>
        <span class="a-title">{t('welcome.browse')}</span>
        <span class="a-desc">{t('welcome.browseDesc')}</span>
      </button>

      <button type="button" class="action" onclick={() => app.openClone()} disabled={blocked}>
        <span class="a-icon"><Icon name="download" size={20} /></span>
        <span class="a-title">{t('welcome.clone')}</span>
        <span class="a-desc">{t('welcome.cloneDesc')}</span>
        <kbd class="a-kbd">{cloneCombo}</kbd>
      </button>

      <form class="action paste" onsubmit={submitManual}>
        <span class="a-icon"><Icon name="clipboard" size={20} /></span>
        <label class="a-title" for="manual-path-{entryId}">{t('welcome.pathLabel')}</label>
        <span class="a-desc">{t('welcome.pasteDesc')}</span>
        <div class="paste-row">
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
          <button type="submit" class="btn" disabled={blocked || manual.trim() === '' || busyPath !== null}>{t('welcome.openButton')}</button>
        </div>
      </form>
    </section>

    {#if openError}
      <div id="open-error" class="open-error" role="alert">
        <Icon name="circle-alert" size={14} />
        <span>{openError.message}</span>
      </div>
    {/if}

    {#if app.recents.length === 0}
      <section class="empty" aria-label={t('welcome.recents')}>
        <div class="empty-art" aria-hidden="true"><Icon name="git-branch" size={24} /></div>
        <h2>{t('welcome.emptyTitle')}</h2>
        <p>{t('welcome.emptyBody')}</p>
      </section>
    {:else}
      {#if favorites.length > 0}
        <section aria-labelledby="fav-h">
          <h2 id="fav-h" class="section-title"><Icon name="star" size={12} filled />{t('welcome.favorites')}<span class="count num">{favorites.length}</span></h2>
          <ul class="grid">
            {#each favorites as r (r.path)}{@render repoCard(r)}{/each}
          </ul>
        </section>
      {/if}
      <section aria-labelledby="rec-h">
        <h2 id="rec-h" class="section-title"><Icon name="history" size={12} />{t('welcome.recents')}<span class="count num">{others.length}</span></h2>
        {#if others.length > 0}
          <ul class="grid">
            {#each others as r (r.path)}{@render repoCard(r)}{/each}
          </ul>
        {:else}
          <p class="muted">{t('welcome.onlyFavorites')}</p>
        {/if}
      </section>
    {/if}

    <footer class="tips">
      <span><kbd>{paletteCombo}</kbd> {t('welcome.tipPalette')}</span>
      <span><kbd>{formatCombo('Mod+T', mac)}</kbd> {t('welcome.tipNewTab')}</span>
    </footer>
  </div>
</div>

<style>
  .welcome {
    position: relative;
    height: 100%;
    overflow: auto;
    isolation: isolate;
  }
  /* Fondo con profundidad: dos focos de luz, una rejilla de puntos desvanecida y el hilo de Ariadna. */
  .bg {
    position: absolute;
    inset: 0;
    z-index: -1;
    overflow: hidden;
    pointer-events: none;
    background:
      radial-gradient(900px 420px at 12% -10%, color-mix(in srgb, var(--accent-brand) 13%, transparent), transparent 70%),
      radial-gradient(760px 420px at 100% 105%, color-mix(in srgb, #8b5cf6 9%, transparent), transparent 70%),
      var(--bg-surface);
  }
  .bg::before {
    content: '';
    position: absolute;
    inset: 0;
    background-image: radial-gradient(circle at 1px 1px, color-mix(in srgb, var(--text) 16%, transparent) 1px, transparent 0);
    background-size: 22px 22px;
    -webkit-mask-image: radial-gradient(700px 360px at 50% 0%, #000, transparent 75%);
    mask-image: radial-gradient(700px 360px at 50% 0%, #000, transparent 75%);
    opacity: 0.55;
  }
  .thread {
    position: absolute;
    top: -90px;
    right: -60px;
    width: 520px;
    height: 520px;
    opacity: 0.2;
  }
  .column {
    display: flex;
    flex-direction: column;
    gap: var(--space-5);
    width: min(1040px, 100%);
    margin: 0 auto;
    padding: 44px var(--space-6) var(--space-6);
  }
  .hero {
    display: flex;
    align-items: center;
    gap: var(--space-4);
  }
  h1 {
    font-size: var(--text-2xl);
    font-weight: 700;
    letter-spacing: -0.03em;
    line-height: 1.1;
  }
  .lead {
    margin-top: 4px;
    color: var(--text-muted);
    font-size: var(--text-lg);
  }

  /* ---- Tarjetas de acción ---- */
  .actions {
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    gap: var(--space-3);
  }
  .action {
    position: relative;
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 4px;
    min-height: 148px;
    padding: var(--space-4);
    border: 1px solid var(--border-hover);
    border-radius: 14px;
    background: linear-gradient(180deg, color-mix(in srgb, var(--bg-raised) 92%, var(--text)), var(--bg-raised));
    box-shadow: var(--elev-inset), var(--elev-1);
    color: var(--text);
    text-align: left;
    transition:
      transform var(--dur) var(--ease-out),
      border-color var(--dur) var(--ease),
      box-shadow var(--dur) var(--ease),
      background-color var(--dur) var(--ease);
  }
  .action:hover:not(:disabled) {
    transform: translateY(-2px);
    border-color: color-mix(in srgb, var(--accent) 45%, var(--border-hover));
    box-shadow: var(--elev-inset), var(--elev-2), 0 0 0 1px color-mix(in srgb, var(--accent) 12%, transparent);
  }
  .action:active:not(:disabled) {
    transform: translateY(0);
  }
  .action:disabled {
    opacity: 0.5;
  }
  .action.primary {
    border-color: color-mix(in srgb, var(--accent) 40%, var(--border-hover));
    background:
      radial-gradient(220px 120px at 0% 0%, color-mix(in srgb, var(--accent-brand) 20%, transparent), transparent 75%),
      linear-gradient(180deg, color-mix(in srgb, var(--bg-raised) 92%, var(--text)), var(--bg-raised));
  }
  .a-icon {
    display: grid;
    place-items: center;
    width: 40px;
    height: 40px;
    margin-bottom: var(--space-2);
    border: 1px solid var(--border-hover);
    border-radius: 12px;
    background: var(--bg-muted);
    color: var(--text-muted);
    transition: color var(--dur) var(--ease), background var(--dur) var(--ease);
  }
  .action:hover:not(:disabled) .a-icon {
    color: var(--accent);
    border-color: color-mix(in srgb, var(--accent) 35%, transparent);
  }
  .primary .a-icon {
    border: none;
    background: var(--grad-accent);
    color: var(--accent-fg);
    box-shadow: inset 0 1px 0 rgb(255 255 255 / 0.3), var(--glow);
  }
  .primary:hover:not(:disabled) .a-icon {
    color: var(--accent-fg);
  }
  .a-title {
    font-size: var(--text-lg);
    font-weight: 650;
    letter-spacing: -0.015em;
  }
  .a-desc {
    color: var(--text-muted);
    font-size: var(--text-sm);
    line-height: 1.4;
  }
  .a-kbd {
    position: absolute;
    top: var(--space-3);
    right: var(--space-3);
  }
  .paste {
    cursor: default;
  }
  .paste:hover {
    transform: none;
  }
  .paste-row {
    display: flex;
    gap: var(--space-2);
    width: 100%;
    margin-top: auto;
    padding-top: var(--space-2);
  }
  .paste-row .field {
    min-width: 0;
    font-size: var(--text-sm);
  }
  .open-error {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    margin-top: calc(-1 * var(--space-2));
    color: var(--danger);
    font-size: var(--text-sm);
  }

  /* ---- Favoritos y recientes ---- */
  .section-title {
    display: flex;
    align-items: center;
    gap: 6px;
    margin-bottom: var(--space-3);
    color: var(--text-muted);
    font-size: var(--text-xs);
    font-weight: 650;
    letter-spacing: 0.07em;
    text-transform: uppercase;
  }
  .section-title :global(svg) {
    color: var(--text-subtle);
  }
  .count {
    padding: 0 6px;
    border-radius: var(--pill);
    background: var(--bg-muted);
    color: var(--text-muted);
    font-size: var(--text-2xs);
    letter-spacing: 0;
  }
  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(280px, 1fr));
    gap: var(--space-3);
    margin: 0;
    padding: 0;
    list-style: none;
  }
  .repo {
    --tone: hsl(var(--hue) 78% 66%);
    position: relative;
    animation: rise-in var(--dur-slow) var(--ease-out) backwards;
  }
  :global(:root[data-theme='light']) .repo {
    --tone: hsl(var(--hue) 70% 46%);
  }
  .open {
    display: flex;
    flex-direction: column;
    gap: 6px;
    width: 100%;
    height: 100%;
    min-height: 96px;
    padding: var(--space-3) var(--space-4) var(--space-3);
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    background: var(--bg-raised);
    box-shadow: var(--elev-inset);
    text-align: left;
    transition:
      transform var(--dur) var(--ease-out),
      border-color var(--dur) var(--ease),
      box-shadow var(--dur) var(--ease);
  }
  .open:hover:not(:disabled) {
    transform: translateY(-1px);
    border-color: color-mix(in srgb, var(--tone) 50%, var(--border-hover));
    box-shadow: var(--elev-inset), var(--elev-1);
  }
  .open:disabled {
    opacity: 0.55;
  }
  .top {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    padding-right: 52px;
  }
  .dot {
    display: grid;
    place-items: center;
    width: 10px;
    height: 10px;
    flex: none;
    border-radius: 50%;
    background: var(--tone);
    box-shadow: 0 0 0 3px color-mix(in srgb, var(--tone) 20%, transparent);
  }
  .dot :global(svg) {
    color: var(--bg-surface);
  }
  .name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: var(--text-lg);
    font-weight: 650;
    letter-spacing: -0.01em;
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
    align-items: center;
    gap: var(--space-2);
    margin-top: auto;
    padding-top: 4px;
    color: var(--text-subtle);
    font-size: var(--text-xs);
  }
  .branch {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    max-width: 55%;
    padding: 1px 7px 1px 6px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    border-radius: var(--pill);
    background: var(--bg-muted);
    color: var(--text-muted);
    font-size: var(--text-2xs);
  }
  .chg {
    padding: 1px 7px;
    border-radius: var(--pill);
    background: var(--accent-soft);
    color: var(--accent);
    font-weight: 600;
  }
  .when {
    margin-left: auto;
    white-space: nowrap;
  }
  .icon {
    position: absolute;
    top: 8px;
    display: grid;
    place-items: center;
    width: 24px;
    height: 24px;
    padding: 0;
    border: none;
    border-radius: 7px;
    background: transparent;
    color: var(--text-subtle);
    transition:
      background-color var(--dur-fast) var(--ease),
      color var(--dur-fast) var(--ease),
      opacity var(--dur-fast) var(--ease),
      transform var(--dur-fast) var(--ease-out);
  }
  .icon:hover {
    background: var(--bg-hover);
    color: var(--text);
  }
  .star {
    right: 8px;
  }
  .star.on {
    color: #f5b544;
  }
  .star:active {
    transform: scale(0.85);
  }
  .forget {
    right: 34px;
    opacity: 0;
  }
  .repo:hover .forget,
  .forget:focus-visible {
    opacity: 1;
  }

  /* ---- Estado vacío ---- */
  .empty {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: var(--space-2);
    padding: var(--space-6) var(--space-5);
    border: 1px dashed var(--border-hover);
    border-radius: var(--radius-lg);
    text-align: center;
    color: var(--text-muted);
  }
  .empty-art {
    display: grid;
    place-items: center;
    width: 52px;
    height: 52px;
    margin-bottom: var(--space-1);
    border-radius: 16px;
    background: var(--grad-accent-soft);
    border: 1px solid color-mix(in srgb, var(--accent) 30%, transparent);
    color: var(--accent);
  }
  .empty h2 {
    color: var(--text);
    font-size: var(--text-lg);
    font-weight: 650;
  }
  .empty p {
    max-width: 52ch;
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
  @media (max-width: 900px) {
    .actions {
      grid-template-columns: 1fr;
    }
    .action {
      min-height: 0;
    }
    .column {
      padding: var(--space-5) var(--space-4);
    }
    .thread {
      display: none;
    }
  }
</style>
