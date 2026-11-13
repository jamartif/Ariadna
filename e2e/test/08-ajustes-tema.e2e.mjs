//! Flujo 8 · ajustes: abrir el diálogo y cambiar el tema.

import assert from 'node:assert/strict';

import { getBrowser, waitInPage } from '../lib/session.mjs';
import { ensureRepoOpen } from '../lib/flows.mjs';
import { click, currentTheme, waitSettingsSaved } from '../lib/ui.mjs';

describe('Flujo 8 · ajustes y tema', () => {
  it('abre los ajustes y cambia a tema oscuro de forma persistente', async () => {
    const b = getBrowser();
    await ensureRepoOpen();

    await click('Ajustes');
    await waitInPage(() => !!document.querySelector('[role="dialog"]'), [], { message: 'diálogo de ajustes' });

    const title = await b.execute(() => {
      const dlg = document.querySelector('[role="dialog"]');
      const labelled = dlg?.getAttribute('aria-labelledby');
      return labelled ? document.getElementById(labelled)?.textContent ?? '' : dlg?.innerText?.slice(0, 40);
    });
    assert.match(String(title), /Ajustes/, 'el diálogo debería titularse «Ajustes»');

    await click('Oscuro');
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

    // El cambio se confirma con «Guardar» (asíncrono: se guarda en settings.json).
    await click('Guardar', { within: '[role="dialog"]' });
    await waitSettingsSaved();
    await click('Cerrar', { within: '[role="dialog"]' });
    await b.waitUntil(async () => !(await b.execute(() => !!document.querySelector('[role="dialog"]'))), {
      timeout: 15_000,
      interval: 200,
      timeoutMsg: 'el diálogo no se cerró tras guardar'
    });
    assert.equal(await currentTheme(), 'dark', 'el tema debe seguir aplicado tras cerrar los ajustes');
  });

  it('vuelve al tema claro y lo aplica sin recargar', async () => {
    const b = getBrowser();
    await click('Ajustes');
    await waitInPage(() => !!document.querySelector('[role="dialog"]'), [], { message: 'diálogo de ajustes' });
    await click('Claro');
    await waitInPage(() => document.documentElement.getAttribute('data-theme') === 'light', [], {
      message: 'tema claro aplicado'
    });
    await click('Guardar', { within: '[role="dialog"]' });
    await waitSettingsSaved();
    await click('Cerrar', { within: '[role="dialog"]' });
    await b.waitUntil(async () => !(await b.execute(() => !!document.querySelector('[role="dialog"]'))), {
      timeout: 15_000,
      interval: 200,
      timeoutMsg: 'el diálogo no se cerró tras guardar el tema claro'
    });
    assert.equal(await currentTheme(), 'light');
    const welcome = await b.execute(() => document.body.innerText.includes('Bienvenida a Ariadna'));
    assert.ok(!welcome, 'la app no debería volver a la bienvenida al cerrar los ajustes');
  });
});
