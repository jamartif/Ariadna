//! Flujo 8 · ajustes con `Ctrl+,`: abrir el diálogo y cambiar el tema.

import assert from 'node:assert/strict';

import { getBrowser, waitInPage } from '../lib/session.mjs';
import { ensureRepoOpen } from '../lib/flows.mjs';
import { clickSelector, clickTestId, currentTheme, press, waitSettingsSaved } from '../lib/ui.mjs';

/** Abre Ajustes por su atajo (`Ctrl+,`) y espera el diálogo. */
async function openSettings() {
  await press(['Control', ',']);
  await waitInPage(() => !!document.querySelector('[role="dialog"]'), [], { message: 'diálogo de ajustes' });
}

/** Guarda y cierra el diálogo; después ya no debe haber `role="dialog"`. */
async function saveAndClose(b) {
  await clickTestId('settings-save');
  await waitSettingsSaved();
  await clickTestId('settings-close');
  await b.waitUntil(async () => !(await b.execute(() => !!document.querySelector('[role="dialog"]'))), {
    timeout: 15_000,
    interval: 200,
    timeoutMsg: 'el diálogo no se cerró tras guardar'
  });
}

describe('Flujo 8 · ajustes y tema', () => {
  it('abre los ajustes con Ctrl+, y cambia a tema oscuro de forma persistente', async () => {
    const b = getBrowser();
    await ensureRepoOpen();

    await openSettings();
    const title = await b.execute(() => document.getElementById('settings-title')?.textContent ?? '');
    assert.ok(title.trim().length > 0, 'el diálogo debería tener título');

    // Las opciones de tema son radios con valores estables (`light`, `dark`, `system`).
    await clickSelector('input[name="set-theme"][value="dark"]');
    await waitInPage(() => document.documentElement.getAttribute('data-theme') === 'dark', [], {
      message: 'tema oscuro aplicado'
    });
    assert.equal(await currentTheme(), 'dark');

    // Se guarda en las preferencias locales del frontend.
    const stored = await b.execute(() => {
      try {
        return JSON.parse(localStorage.getItem('ariadna.settings.v1') ?? '{}').theme ?? null;
      } catch {
        return null;
      }
    });
    assert.equal(stored, 'dark', 'el tema debería quedar guardado en localStorage');

    await saveAndClose(b);
    assert.equal(await currentTheme(), 'dark', 'el tema debe seguir aplicado tras cerrar los ajustes');
  });

  it('vuelve al tema claro y lo aplica sin recargar', async () => {
    const b = getBrowser();
    await openSettings();
    await clickSelector('input[name="set-theme"][value="light"]');
    await waitInPage(() => document.documentElement.getAttribute('data-theme') === 'light', [], {
      message: 'tema claro aplicado'
    });
    await saveAndClose(b);
    assert.equal(await currentTheme(), 'light');
    const welcome = await b.execute(() => !!document.querySelector('[data-testid="welcome"]'));
    assert.ok(!welcome, 'la app no debería volver a la bienvenida al cerrar los ajustes');
  });
});
