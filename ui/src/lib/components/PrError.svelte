<script lang="ts">
  import { onDestroy } from 'svelte';
  import { t } from '$lib/i18n';
  import type { ApiError } from '$lib/api/types';
  import { settingsUi } from '$lib/stores/settingsPanel.svelte';
  import { errorKind, retrySeconds } from '$lib/util/pr';
  import Banner from './Banner.svelte';
  import Icon from './Icon.svelte';

  /**
   * Error tipado de una operación de PR (spec PR-10). Cada clase tiene su aviso y su salida:
   * AUTH (qué permiso falta + ayuda del PAT en Ajustes), NOT_FOUND, POLICY (el motivo del
   * proveedor), RATE_LIMITED (cuenta atrás y reintento), NETWORK y CONFLICT (actualizar).
   */
  interface Props {
    error: ApiError;
    /** Vuelve a pedir lo que falló. */
    onretry?: () => void;
    /** Cierra el aviso (solo errores de acciones, no de carga). */
    ondismiss?: () => void;
    /** Vuelve a la lista (NOT_FOUND). */
    onback?: () => void;
  }
  let { error, onretry, ondismiss, onback }: Props = $props();

  const kind = $derived(errorKind(error));
  const severity = $derived<'error' | 'warning' | 'info'>(kind === 'policy' || kind === 'rateLimited' || kind === 'conflict' ? 'warning' : 'error');
  const title = $derived(
    kind === 'auth'
      ? t('pr.error.auth')
      : kind === 'notFound'
        ? t('pr.error.notFound')
        : kind === 'policy'
          ? t('pr.error.policy')
          : kind === 'rateLimited'
            ? t('pr.error.rateLimited')
            : kind === 'network'
              ? t('pr.error.network')
              : kind === 'conflict'
                ? t('pr.error.conflict')
                : t('pr.error.generic')
  );

  // Cuenta atrás de RATE_LIMITED: el botón de reintento se activa al llegar a 0.
  let left = $state(0);
  let timer: ReturnType<typeof setInterval> | null = null;
  $effect(() => {
    const e = error;
    if (timer) clearInterval(timer);
    timer = null;
    if (errorKind(e) !== 'rateLimited') {
      left = 0;
      return;
    }
    left = retrySeconds(e);
    timer = setInterval(() => {
      left = Math.max(0, left - 1);
      if (left === 0 && timer) {
        clearInterval(timer);
        timer = null;
      }
    }, 1000);
  });
  onDestroy(() => {
    if (timer) clearInterval(timer);
  });

  const waiting = $derived(kind === 'rateLimited' && left > 0);
</script>

<div class="prerr" data-kind={kind}>
  <Banner kind={severity} {title} message={error.message} detail={kind === 'auth' ? null : error.detail && kind !== 'rateLimited' ? error.detail : null}>
    {#if kind === 'auth'}
      <p class="perm">
        <Icon name="key" size={13} />
        {#if error.detail}
          {t('pr.error.authPermission', { permission: error.detail })}
        {:else}
          {t('pr.error.authNoPermission')}
        {/if}
      </p>
    {/if}
    {#if kind === 'rateLimited'}
      <p class="perm" role="timer" aria-live="off">
        <Icon name="clock" size={13} />
        {waiting ? t('pr.error.waitSeconds', { n: left }) : t('pr.error.waitDone')}
      </p>
    {/if}
    {#snippet actions()}
      {#if kind === 'auth'}
        <button type="button" class="btn" onclick={() => settingsUi.open('accounts')}>
          <Icon name="key" size={14} />{t('pr.error.authHelp')}
        </button>
      {/if}
      {#if kind === 'notFound' && onback}
        <button type="button" class="btn" onclick={onback}><Icon name="chevron-left" size={14} />{t('pr.error.backToList')}</button>
      {/if}
      {#if onretry}
        <button type="button" class="btn" disabled={waiting} onclick={onretry}>
          <Icon name="refresh-cw" size={14} />{kind === 'conflict' ? t('pr.error.reload') : t('banner.retry')}
        </button>
      {/if}
      {#if ondismiss}
        <button type="button" class="btn btn-ghost" onclick={ondismiss}>{t('pr.error.dismiss')}</button>
      {/if}
    {/snippet}
  </Banner>
</div>

<style>
  .prerr {
    min-width: 0;
  }
  .perm {
    display: flex;
    align-items: center;
    gap: 6px;
    margin: var(--space-1) 0 0;
    font-size: var(--text-sm);
    color: var(--text-muted);
  }
</style>
