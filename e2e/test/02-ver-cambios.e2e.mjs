//! Flujo 2 · ver los cambios del árbol de trabajo y el diff de un archivo.

import assert from 'node:assert/strict';
import path from 'node:path';

import { getBrowser, waitInPage } from '../lib/session.mjs';
import { showPanel } from '../lib/flows.mjs';
import { getFixture } from '../lib/state.mjs';
import { hasText, selectFile } from '../lib/ui.mjs';

describe('Flujo 2 · ver cambios y diff', () => {
  it('lista los archivos modificados y no rastreados', async () => {
    const fx = getFixture();
    await showPanel('Cambios');

    // La vista puede ser de lista o de árbol: se comprueba por nombre de archivo.
    for (const file of [...fx.modificados, fx.nuevo, fx.preparado]) {
      const base = path.basename(file);
      await waitInPage((name) => document.body.innerText.includes(name), [base], {
        message: `fila del archivo ${file}`
      });
    }

    const text = await getBrowser().execute(() => document.body.innerText);
    assert.match(text, /Preparados/, 'falta la sección de archivos preparados');
    assert.match(text, /Sin preparar/, 'falta la sección de cambios sin preparar');
    assert.match(text, /Sin seguimiento/, 'falta la sección de archivos sin seguimiento');
    assert.ok(await hasText('4 archivos con cambios'), 'el recuento de cambios no coincide');
  });

  it('muestra el diff del archivo seleccionado', async () => {
    await selectFile('src/app.js');
    await waitInPage(
      () => {
        const pane = document.querySelector('[aria-label="Diferencias"]');
        return (pane?.innerText ?? '').includes('export const version = 2');
      },
      [],
      { message: 'diff de src/app.js' }
    );

    const diffText = await getBrowser().execute(() => {
      const pane = document.querySelector('[aria-label="Diferencias"]');
      return pane ? pane.innerText : '';
    });
    assert.match(diffText, /version = 2/, 'el panel de diferencias debería incluir la línea añadida');
    assert.match(diffText, /Hola, \$\{nombre\}/, 'el diff debería mostrar el contexto del archivo');
  });
});
