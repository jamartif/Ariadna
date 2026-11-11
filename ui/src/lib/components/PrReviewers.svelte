<script lang="ts">
  import { t } from '$lib/i18n';
  import type { PrReviewer } from '$lib/api/prTypes';
  import type { RemoteProviderId } from '$lib/api/types';
  import { initials, VOTE_ICON, VOTE_TONE, voteLabel } from '$lib/util/pr';
  import Icon from './Icon.svelte';

  /** Revisores con su voto: iniciales + icono del voto + texto (para lectores de pantalla). */
  interface Props {
    reviewers: PrReviewer[];
    provider?: RemoteProviderId;
    /** Máximo de revisores a mostrar en modo compacto; el resto va en «+n». */
    max?: number;
    /** Lista con nombre y voto en texto (detalle). */
    full?: boolean;
  }
  let { reviewers, provider, max = 4, full = false }: Props = $props();

  const shown = $derived(full ? reviewers : reviewers.slice(0, max));
  const hidden = $derived(full ? 0 : Math.max(0, reviewers.length - max));
</script>

{#if reviewers.length === 0}
  <span class="none">{t('pr.reviewers.none')}</span>
{:else}
  <span role="list" class="revs" class:full aria-label={t('pr.reviewers.label')}>
    {#each shown as r (r.user.id)}
      {@const label = t(voteLabel(r.vote, provider))}
      <span role="listitem" class="rev" data-tone={VOTE_TONE[r.vote]} title="{r.user.displayName}: {label}{r.required ? ` · ${t('pr.reviewers.required')}` : ''}">
        <span class="avatar" aria-hidden="true">{initials(r.user.displayName)}<span class="vote"><Icon name={VOTE_ICON[r.vote]} size={10} /></span></span>
        {#if full}
          <span class="name">{r.user.displayName}</span>
          {#if r.required}<span class="req">{t('pr.reviewers.required')}</span>{/if}
          <span class="vlabel">{label}</span>
        {:else}
          <span class="sr-only">{r.user.displayName}: {label}</span>
        {/if}
      </span>
    {/each}
    {#if hidden > 0}<span role="listitem" class="more" title={t('pr.reviewers.more', { n: hidden })}><span aria-hidden="true">+{hidden}</span><span class="sr-only">{t('pr.reviewers.more', { n: hidden })}</span></span>{/if}
  </span>
{/if}

<style>
  .none {
    font-size: var(--text-xs);
    color: var(--text-subtle);
  }
  .revs {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
    margin: 0;
    padding: 0;
    list-style: none;
  }
  .revs.full {
    flex-direction: column;
    gap: var(--space-1);
  }
  .rev {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    min-width: 0;
    font-size: var(--text-sm);
  }
  .avatar {
    position: relative;
    display: grid;
    place-items: center;
    flex: none;
    width: 22px;
    height: 22px;
    border-radius: 50%;
    background: var(--bg-muted);
    color: var(--text);
    font-size: 9px;
    font-weight: 700;
    border: 1px solid var(--border-strong);
  }
  .vote {
    position: absolute;
    right: -5px;
    bottom: -5px;
    display: grid;
    place-items: center;
    width: 13px;
    height: 13px;
    border-radius: 50%;
    background: var(--bg-surface);
    color: var(--text-muted);
  }
  .rev[data-tone='success'] .vote {
    color: var(--success);
  }
  .rev[data-tone='danger'] .vote {
    color: var(--danger);
  }
  .rev[data-tone='warning'] .vote {
    color: var(--warning);
  }
  .rev[data-tone='info'] .vote {
    color: var(--info);
  }
  .name {
    font-weight: 600;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .req {
    font-size: var(--text-2xs);
    color: var(--text-muted);
    border: 1px solid var(--border-strong);
    border-radius: var(--pill);
    padding: 0 6px;
  }
  .vlabel {
    margin-left: auto;
    color: var(--text-muted);
    font-size: var(--text-xs);
  }
  .more {
    display: grid;
    place-items: center;
    min-width: 22px;
    height: 22px;
    padding: 0 4px;
    border-radius: var(--pill);
    background: var(--bg-muted);
    color: var(--text-muted);
    font-size: 10px;
    font-weight: 650;
  }
</style>
