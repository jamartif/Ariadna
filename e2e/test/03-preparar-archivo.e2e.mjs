//! Flujo 3 · preparar un archivo (índice real, comprobado con git).

import assert from 'node:assert/strict';

import { waitInPage } from '../lib/session.mjs';
import { showPanel } from '../lib/flows.mjs';
import { getFixture } from '../lib/state.mjs';
import { clickSelector } from '../lib/ui.mjs';
import { repoState } from '../lib/fixture.mjs';

describe('Flujo 3 · preparar un archivo', () => {
  it('pasa src/app.js al índice y la interfaz lo refleja', async () => {
    const fx = getFixture();
    await showPanel('Cambios');

    assert.ok(!repoState(fx.repo).staged.includes('src/app.js'), 'el archivo no debería estar preparado de antemano');

    await clickSelector('[aria-label="Preparar src/app.js"]');

    await waitInPage(
      (label) =>
        Array.from(document.querySelectorAll('[aria-label]')).some(
          (el) => (el.getAttribute('aria-label') ?? '') === label
        ),
      ['Quitar de preparados src/app.js'],
      { message: 'el archivo aparece como preparado' }
    );

    const state = repoState(fx.repo);
    assert.ok(state.staged.includes('src/app.js'), `git no ve el archivo preparado: ${state.staged}`);
    assert.ok(!state.unstaged.includes('src/app.js'), 'el archivo no debería seguir en «sin preparar»');
    assert.deepEqual(state.staged.sort(), [fx.preparado, 'src/app.js'].sort(), 'solo esos dos archivos deben estar preparados');
  });
});
