<script lang="ts">
  import { onMount } from 'svelte';
  import { t } from '$lib/i18n';
  import type { AiChoice, AiProfileKind } from '$lib/api/types';
  import type { AiController } from '$lib/stores/ai.svelte';
  import { toasts } from '$lib/stores/toasts.svelte';
  import { focusTrap } from '$lib/util/focusTrap';
  import Icon from './Icon.svelte';

  let { ai, onclose }: { ai: AiController; onclose: () => void } = $props();

  // Borrador local: la elección de proveedor no se guarda hasta pulsar el botón principal.
  let profile = $state<AiProfileKind>('personal');
  let choice = $state<AiChoice>('auto');
  let saving = $state(false);
  let seeded = false;

  // Se parte de lo guardado en cuanto se conoce (una sola vez: luego manda el borrador).
  $effect(() => {
    const s = ai.state;
    if (s && !seeded) {
      seeded = true;
      profile = s.profile.kind;
      choice = s.choice;
    }
  });

  const enabled = $derived(ai.state?.enabled === true);
  const providers = $derived(ai.state?.providers ?? []);

  onMount(() => {
    if (!ai.state) void ai.loadState();
  });

  // Al cambiar de perfil Rust devuelve otro conjunto de proveedores habilitados.
  async function pickProfile(kind: AiProfileKind) {
    profile = kind;
    if (ai.state && ai.state.profile.kind !== kind) await ai.setProfile(kind);
  }

  async function save(turnOn: boolean) {
    saving = true;
    try {
      const ok = await ai.saveSettings({ enabled: turnOn, profile, choice });
      if (ok) {
        toasts.success(t('ai.dialog.saved'));
        onclose();
      }
    } finally {
      saving = false;
    }
  }

  function key(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      e.stopPropagation();
      onclose();
    }
  }

  function availabilityText(a: string): string {
    return a === 'available' ? t('ai.dialog.availability.available') : a === 'missing' ? t('ai.dialog.availability.missing') : t('ai.dialog.availability.failed');
  }
</script>

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="backdrop" onclick={onclose}>
  <div
    class="dialog float"
    role="dialog"
    aria-modal="true"
    aria-labelledby="ai-dialog-title"
    aria-describedby="ai-dialog-sub"
    tabindex="-1"
    use:focusTrap
    onclick={(e) => e.stopPropagation()}
    onkeydown={key}
  >
    <header class="head">
      <span class="badge"><Icon name="sparkles" size={18} /></span>
      <div class="titles">
        <h2 id="ai-dialog-title">{t('ai.dialog.title')}</h2>
        <p id="ai-dialog-sub">{t('ai.dialog.subtitle')}</p>
      </div>
      <button type="button" class="btn btn-ghost btn-icon" onclick={onclose} aria-label={t('ai.dialog.close')}>
        <Icon name="x" size={16} />
      </button>
    </header>

    <div class="body">
      <section aria-labelledby="ai-what">
        <h3 id="ai-what">{t('ai.dialog.whatTitle')}</h3>
        <ul>
          <li>{t('ai.dialog.what1')}</li>
          <li>{t('ai.dialog.what2')}</li>
          <li>{t('ai.dialog.what3')}</li>
          <li>{t('ai.dialog.what4')}</li>
        </ul>
        <p class="note">{t('ai.dialog.whom')}</p>
      </section>

      <fieldset class="group">
        <legend>{t('ai.dialog.profile')}</legend>
        <div class="seg" role="radiogroup" aria-label={t('ai.dialog.profile')}>
          {#each ['personal', 'empresa'] as const as kind (kind)}
            <label class="opt" class:on={profile === kind}>
              <input type="radio" name="ai-profile" value={kind} checked={profile === kind} onchange={() => void pickProfile(kind)} />
              <span>{t(kind === 'personal' ? 'ai.dialog.profile.personal' : 'ai.dialog.profile.empresa')}</span>
            </label>
          {/each}
        </div>
        <p class="hint">{t('ai.dialog.profileHint')}</p>
      </fieldset>

      <fieldset class="group">
        <legend>{t('ai.dialog.provider')}</legend>
        <div class="providers" role="radiogroup" aria-label={t('ai.dialog.provider')}>
          <label class="prov" class:on={choice === 'auto'}>
            <input type="radio" name="ai-provider" value="auto" checked={choice === 'auto'} onchange={() => (choice = 'auto')} aria-labelledby="ai-p-auto" aria-describedby="ai-p-auto-meta" />
            <span class="pname" id="ai-p-auto">{t('ai.dialog.provider.auto')}</span>
            <span class="pmeta" id="ai-p-auto-meta">{t('ai.dialog.provider.autoHint')}</span>
          </label>
          {#each providers as p (p.id)}
            <label class="prov" class:on={choice === p.id} class:off={!p.enabled}>
              <input type="radio" name="ai-provider" value={p.id} checked={choice === p.id} onchange={() => (choice = p.id as AiChoice)} aria-labelledby="ai-p-{p.id}" aria-describedby="ai-p-{p.id}-meta" />
              <span class="pname" id="ai-p-{p.id}">{p.displayName}</span>
              <span class="pmeta" id="ai-p-{p.id}-meta">
                <span class="state" data-state={p.availability}>{availabilityText(p.availability)}{p.version ? ` · ${p.version}` : ''}</span>
                · {!p.enabled ? t('ai.dialog.disabledByProfile') : p.consented ? t('ai.dialog.consented') : t('ai.dialog.notConsented')}
              </span>
            </label>
          {/each}
          {#if providers.length === 0}
            <p class="hint">{t('ai.dialog.loading')}</p>
          {/if}
        </div>
      </fieldset>
    </div>

    <footer class="foot">
      {#if enabled}
        <button type="button" class="btn btn-ghost" onclick={() => void save(false)} disabled={saving}>{t('ai.dialog.disable')}</button>
      {/if}
      <span class="spacer"></span>
      <button type="button" class="btn" onclick={onclose} disabled={saving}>{t('ai.dialog.cancel')}</button>
      <button type="button" class="btn btn-primary" data-autofocus onclick={() => void save(true)} disabled={saving}>
        {#if saving}<Icon name="loader" size={14} spin />{/if}
        {enabled ? t('ai.dialog.save') : t('ai.dialog.enable')}
      </button>
    </footer>
  </div>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 90;
    display: flex;
    align-items: flex-start;
    justify-content: center;
    padding: 7vh var(--space-4) var(--space-4);
    background: var(--bg-overlay);
    backdrop-filter: blur(3px);
    animation: fade-in var(--dur) var(--ease);
    overflow: auto;
  }
  .dialog {
    width: min(560px, 100%);
    border-radius: 16px;
    overflow: hidden;
  }
  .head {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    padding: var(--space-4) var(--space-4) var(--space-3);
  }
  .badge {
    display: grid;
    place-items: center;
    width: 36px;
    height: 36px;
    flex: none;
    border-radius: 11px;
    background: var(--grad-accent-soft);
    border: 1px solid color-mix(in srgb, var(--accent) 35%, transparent);
    color: var(--accent);
  }
  .titles {
    flex: 1;
    min-width: 0;
  }
  h2 {
    font-size: var(--text-lg);
    font-weight: 650;
    letter-spacing: -0.015em;
  }
  .titles p {
    color: var(--text-muted);
    font-size: var(--text-sm);
  }
  .body {
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
    padding: var(--space-2) var(--space-4) var(--space-3);
  }
  h3,
  legend {
    margin-bottom: 6px;
    padding: 0;
    font-size: var(--text-xs);
    font-weight: 600;
    letter-spacing: 0.05em;
    text-transform: uppercase;
    color: var(--text-muted);
  }
  ul {
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding-left: 1.1em;
    font-size: var(--text-sm);
    color: var(--text);
  }
  .note,
  .hint {
    margin-top: var(--space-2);
    font-size: var(--text-xs);
    color: var(--text-subtle);
  }
  .group {
    border: 0;
    padding: 0;
    min-width: 0;
  }
  .seg {
    display: inline-flex;
    padding: 2px;
    gap: 2px;
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    background: var(--bg-subtle);
  }
  .opt {
    position: relative;
    display: inline-flex;
    align-items: center;
    min-height: 30px;
    padding: 0 var(--space-4);
    border-radius: calc(var(--radius-md) - 2px);
    font-size: var(--text-sm);
    color: var(--text-muted);
    cursor: pointer;
  }
  .opt.on {
    background: var(--bg-raised);
    color: var(--text);
    font-weight: 600;
    box-shadow: var(--elev-1);
  }
  .opt input,
  .prov input {
    position: absolute;
    inset: 0;
    opacity: 0;
    margin: 0;
    cursor: pointer;
  }
  .opt:has(input:focus-visible),
  .prov:has(input:focus-visible) {
    outline: 2px solid var(--focus-ring);
    outline-offset: 2px;
  }
  .providers {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .prov {
    position: relative;
    display: grid;
    grid-template-columns: 7.5em 1fr;
    align-items: baseline;
    gap: var(--space-3);
    padding: var(--space-2) var(--space-3);
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    cursor: pointer;
  }
  .prov:hover {
    border-color: var(--border-hover);
  }
  .prov.on {
    border-color: var(--accent);
    background: var(--accent-soft);
  }
  .prov.off .pname {
    color: var(--text-subtle);
  }
  .pname {
    font-weight: 600;
    font-size: var(--text-sm);
  }
  .pmeta {
    font-size: var(--text-xs);
    color: var(--text-muted);
  }
  .state[data-state='available'] {
    color: var(--success);
  }
  .state[data-state='missing'] {
    color: var(--text-subtle);
  }
  .state[data-state='failed'] {
    color: var(--warning);
  }
  .foot {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    padding: var(--space-3) var(--space-4) var(--space-4);
    border-top: 1px solid var(--border);
  }
  .spacer {
    flex: 1;
  }
  @media (forced-colors: active) {
    .prov.on,
    .opt.on {
      outline: 2px solid Highlight;
    }
  }
</style>
