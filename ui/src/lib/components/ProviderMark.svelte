<script lang="ts">
  import type { RemoteProvider } from '$lib/util/remote';
  import Icon from './Icon.svelte';

  let { provider, size = 22 }: { provider: RemoteProvider; size?: number } = $props();

  const LABEL: Record<RemoteProvider, string> = { azure: 'Az', github: 'GH', gitlab: 'GL', bitbucket: 'BB', other: '' };
</script>

<!-- Marca neutra por proveedor (sin logotipos de terceros): color + iniciales. -->
<span class="mark" data-provider={provider} style:width="{size}px" style:height="{size}px" style:font-size="{Math.round(size * 0.42)}px" aria-hidden="true">
  {#if provider === 'other'}<Icon name="git-fork" size={Math.round(size * 0.6)} />{:else}{LABEL[provider]}{/if}
</span>

<style>
  .mark {
    display: inline-grid;
    place-items: center;
    flex: none;
    border-radius: 6px;
    color: #fff;
    font-weight: 700;
    letter-spacing: -0.02em;
    box-shadow: inset 0 1px 0 rgb(255 255 255 / 0.25);
  }
  [data-provider='azure'] {
    background: linear-gradient(180deg, #2b95e8, #0a64b8);
  }
  [data-provider='github'] {
    background: linear-gradient(180deg, #4a5565, #1f2733);
  }
  [data-provider='gitlab'] {
    background: linear-gradient(180deg, #ff8a4c, #d9531a);
  }
  [data-provider='bitbucket'] {
    background: linear-gradient(180deg, #4b95ff, #1d5fd6);
  }
  [data-provider='other'] {
    background: var(--bg-muted);
    color: var(--text-muted);
    box-shadow: none;
    border: 1px solid var(--border-hover);
  }
</style>
