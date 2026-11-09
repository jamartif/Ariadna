<script lang="ts">
  import { t } from '$lib/i18n';
  import { DIFF_FONT_MAX, DIFF_FONT_MIN, type ThemePref } from '$lib/api/types';
  import type { SettingsPanel } from '$lib/stores/settingsPanel.svelte';

  let { panel }: { panel: SettingsPanel } = $props();

  const THEMES: { id: ThemePref; label: 'settings.general.themeSystem' | 'settings.general.themeLight' | 'settings.general.themeDark' }[] = [
    { id: 'system', label: 'settings.general.themeSystem' },
    { id: 'light', label: 'settings.general.themeLight' },
    { id: 'dark', label: 'settings.general.themeDark' }
  ];

  const draft = $derived(panel.draft);

  function onSize(e: Event) {
    const v = (e.currentTarget as HTMLInputElement).value;
    panel.setDiffFontSize(v.trim() === '' ? Number.NaN : Number(v));
  }
</script>

{#if draft}
  <div class="sx">
    <div class="group">
      <label class="lbl" for="set-lang">{t('settings.general.language')}</label>
      <select id="set-lang" class="field" value={draft.language} disabled aria-describedby="set-lang-hint">
        <option value="es">{t('settings.general.languageEs')}</option>
      </select>
      <p class="hint" id="set-lang-hint">{t('settings.general.languageHint')}</p>
    </div>

    <fieldset class="group">
      <legend>{t('settings.general.theme')}</legend>
      <div class="seg" role="radiogroup" aria-label={t('settings.general.theme')}>
        {#each THEMES as th (th.id)}
          <label class="opt" class:on={draft.theme === th.id}>
            <input type="radio" name="set-theme" value={th.id} checked={draft.theme === th.id} onchange={() => panel.setTheme(th.id)} />
            <span>{t(th.label)}</span>
          </label>
        {/each}
      </div>
    </fieldset>

    <div class="group">
      <label class="lbl" for="set-diff">{t('settings.general.diffSize')}</label>
      <div class="row size">
        <input
          id="set-diff"
          class="field num"
          style="flex: none; width: 88px"
          type="number"
          inputmode="numeric"
          min={DIFF_FONT_MIN}
          max={DIFF_FONT_MAX}
          step="1"
          value={Number.isNaN(draft.diffFontSize) ? '' : draft.diffFontSize}
          oninput={onSize}
          aria-invalid={panel.fontProblem}
          aria-describedby="set-diff-hint"
        />
        <span class="unit">px</span>
      </div>
      <p class="hint" id="set-diff-hint" class:bad={panel.fontProblem} style:color={panel.fontProblem ? 'var(--danger)' : undefined} role={panel.fontProblem ? 'alert' : undefined}>
        {panel.fontProblem
          ? t('settings.general.diffSizeInvalid', { min: DIFF_FONT_MIN, max: DIFF_FONT_MAX })
          : t('settings.general.diffSizeHint', { min: DIFF_FONT_MIN, max: DIFF_FONT_MAX })}
      </p>
      <div class="preview" aria-label={t('settings.general.preview')}>
        <span class="cap">{t('settings.general.preview')}</span>
        <code>{t('settings.general.previewLine')}</code>
      </div>
    </div>
  </div>
{/if}

<style>
  .num {
    width: 88px;
    flex: none;
  }
  .size {
    gap: var(--space-2);
  }
  .unit {
    color: var(--text-muted);
    font-size: var(--text-sm);
  }
  .hint.bad {
    color: var(--danger);
  }
  .preview {
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding: var(--space-3);
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    background: var(--bg-subtle);
  }
  .cap {
    font-size: var(--text-xs);
    color: var(--text-muted);
  }
  code {
    font-family: var(--font-mono);
    font-size: var(--diff-font-size, 13px);
    color: var(--text);
  }
</style>
