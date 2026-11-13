//! Flujo 2 · ver los cambios del árbol de trabajo y el diff de un archivo.

import assert from 'node:assert/strict';

import { getBrowser, waitInPage } from '../lib/session.mjs';
import { showView } from '../lib/flows.mjs';
import { getFixture } from '../lib/state.mjs';
import { fileSection, selectFile } from '../lib/ui.mjs';

describe('Flujo 2 · ver cambios y diff', () => {
  it('lista los archivos modificados y no rastreados', async () => {
    const fx = getFixture();
    await showView('changes');

    // La vista puede ser de lista o de árbol: se comprueba por ruta (la celda lleva `title`).
    for (const file of [...fx.modificados, fx.nuevo, fx.preparado]) {
      await waitInPage(
        (p) =>
          Array.from(document.querySelectorAll('[role="row"] [title]')).some(
            (el) => (el.getAttribute('title') ?? '') === p
          ),
        [file],
        { message: `fila del archivo ${file}` }
      );
    }

    // Las tres secciones se distinguen por `data-section`, no por su texto traducible.
    assert.equal(await fileSection(fx.preparado), 'staged', 'falta la sección de archivos preparados');
    assert.equal(await fileSection('docs/guia.md'), 'unstaged', 'falta la sección de cambios sin preparar');
    assert.equal(await fileSection(fx.nuevo), 'untracked', 'falta la sección de archivos sin seguimiento');

    const rows = await getBrowser().execute(
      () => document.querySelector('[data-testid="changes-summary"]')?.textContent?.trim() ?? ''
    );
    assert.match(rows, /^4\b/, `el resumen del panel debería decir 4 archivos con cambios, dice «${rows}»`);
  });

  it('muestra el diff del archivo seleccionado', async () => {
    await selectFile('src/app.js');
    await waitInPage(
      () => {
        const pane = document.querySelector('[data-testid="diff-region"]');
        return (pane?.innerText ?? '').includes('export const version = 2');
      },
      [],
      { message: 'diff de src/app.js' }
    );

    const diffText = await getBrowser().execute(() => {
      const pane = document.querySelector('[data-testid="diff-region"]');
      return pane ? pane.innerText : '';
    });
    assert.match(diffText, /version = 2/, 'el panel de diferencias debería incluir la línea añadida');
    assert.match(diffText, /Hola, \$\{nombre\}/, 'el diff debería mostrar el contexto del archivo');
  });
});
