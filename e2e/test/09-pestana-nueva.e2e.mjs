//! Flujo 9 · abrir y cerrar una pestaña nueva.

import assert from 'node:assert/strict';

import { getBrowser, waitInPage } from '../lib/session.mjs';
import { ensureRepoOpen } from '../lib/flows.mjs';
import { click, clickSelector, tabCount } from '../lib/ui.mjs';

/** Cierra la pestaña en blanco si existe (la app reutiliza una única pestaña «nueva»). */
async function closeBlankTab() {
  const b = getBrowser();
  return b.execute(() => {
    const tab = Array.from(document.querySelectorAll('[role="tab"][data-tab-id]')).find((el) =>
      /nueva pestaña/i.test((el.getAttribute('aria-label') ?? '').trim())
    );
    if (!tab) return false;
    const wrapper = tab.closest('div');
    const btn = wrapper?.querySelector('button[aria-label^="Cerrar"]');
    if (!btn) return false;
    btn.click();
    return true;
  });
}

describe('Flujo 9 · pestaña nueva', () => {
  it('abre una pestaña en blanco con la bienvenida y la cierra', async () => {
    const b = getBrowser();
    await ensureRepoOpen();

    // La app mantiene una sola pestaña «nueva»: se cierra para partir de cero.
    if (await closeBlankTab()) {
      await waitInPage(() => !document.body.innerText.includes('Bienvenida a Ariadna'), [], {
        message: 'la pestaña en blanco se cerró'
      });
    }
    const before = await tabCount();

    await clickSelector('button.new');

    await waitInPage(
      (n) => document.querySelectorAll('[role="tablist"] [role="tab"][data-tab-id]').length === n + 1,
      [before],
      { message: 'pestaña nueva en la barra' }
    );
    assert.equal(await tabCount(), before + 1);

    await waitInPage(() => document.body.innerText.includes('Bienvenida a Ariadna'), [], {
      message: 'la pestaña nueva muestra la bienvenida'
    });

    // Cierra la pestaña activa desde su botón de cerrar.
    const closed = await b.execute(() => {
      const active = document.querySelector('[role="tab"][aria-selected="true"]');
      const wrapper = active?.closest('div[class*="tab"]');
      const btn = wrapper?.querySelector('button[aria-label^="Cerrar"]');
      if (!btn) return false;
      btn.click();
      return true;
    });
    assert.ok(closed, 'no se encontró el botón de cerrar de la pestaña nueva');

    await waitInPage(
      (n) => document.querySelectorAll('[role="tablist"] [role="tab"][data-tab-id]').length === n,
      [before],
      { message: 'la pestaña vuelve a cerrarse' }
    );
    assert.equal(await tabCount(), before);

    // El repositorio sigue abierto tras cerrar la pestaña en blanco.
    await click('Cambios');
    await waitInPage(() => document.body.innerText.includes('guia.md'), [], {
      message: 'el repositorio sigue en su sitio'
    });
  });
});
