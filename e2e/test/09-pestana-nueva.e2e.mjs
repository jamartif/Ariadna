//! Flujo 9 · abrir y cerrar una pestaña nueva.

import assert from 'node:assert/strict';

import { getBrowser, waitInPage } from '../lib/session.mjs';
import { ensureRepoOpen, showView } from '../lib/flows.mjs';
import { closeTab, clickTestId, fileSection, tabCount, tabs } from '../lib/ui.mjs';

/** Cierra la pestaña en blanco si existe (la app reutiliza una única pestaña «nueva»). */
async function closeBlankTab() {
  const blank = (await tabs()).find((t) => t.kind === 'new');
  if (!blank) return false;
  await closeTab(blank.id);
  return true;
}

describe('Flujo 9 · pestaña nueva', () => {
  it('abre una pestaña en blanco con la bienvenida y la cierra con el teclado', async () => {
    const b = getBrowser();
    await ensureRepoOpen();

    // La app mantiene una sola pestaña «nueva»: se cierra para partir de cero.
    if (await closeBlankTab()) {
      await waitInPage(() => !document.querySelector('[data-testid="welcome"]'), [], {
        message: 'la pestaña en blanco se cerró'
      });
    }
    const before = await tabCount();

    await clickTestId('tab-new');

    await waitInPage(
      (n) => document.querySelectorAll('[role="tab"][data-tab-id]').length === n + 1,
      [before],
      { message: 'pestaña nueva en la barra' }
    );
    assert.equal(await tabCount(), before + 1);

    const blank = (await tabs()).find((t) => t.kind === 'new');
    assert.ok(blank, 'la pestaña nueva debería estar en la barra');
    assert.ok(blank.selected, 'la pestaña nueva debería quedar activa');

    await waitInPage(() => !!document.querySelector('[data-testid="welcome"]'), [], {
      message: 'la pestaña nueva muestra la bienvenida'
    });

    // Se cierra por su vía real: foco en la pestaña y `Supr` (el botón de cerrar
    // no está en el árbol de accesibilidad: un tablist solo admite pestañas).
    const how = await closeTab(blank.id);
    assert.equal(how, 'supr', `la pestaña debería cerrarse con Supr, se cerró con «${how}»`);
    await waitInPage(
      (n) => document.querySelectorAll('[role="tab"][data-tab-id]').length === n,
      [before],
      { message: 'la pestaña vuelve a cerrarse' }
    );
    assert.equal(await tabCount(), before);

    // El repositorio sigue abierto tras cerrar la pestaña en blanco.
    await showView('changes');
    await waitInPage(() => !!document.querySelector('[data-testid="diff-region"]'), [], {
      message: 'el panel de diferencias sigue en su sitio'
    });
    assert.equal(await fileSection('docs/guia.md'), 'unstaged', 'el repositorio sigue mostrando sus cambios');
    assert.ok(await b.execute(() => !!document.querySelector('[data-testid="rail"]')), 'el riel sigue visible');
  });
});
