//! Flujo 3 · preparar un archivo (índice real, comprobado con git).

import assert from 'node:assert/strict';

import { waitInPage } from '../lib/session.mjs';
import { showView } from '../lib/flows.mjs';
import { getFixture } from '../lib/state.mjs';
import { fileStaged, toggleStage } from '../lib/ui.mjs';
import { repoState } from '../lib/fixture.mjs';

describe('Flujo 3 · preparar un archivo', () => {
  it('pasa src/app.js al índice y la interfaz lo refleja', async () => {
    const fx = getFixture();
    await showView('changes');

    assert.ok(!repoState(fx.repo).staged.includes('src/app.js'), 'el archivo no debería estar preparado de antemano');

    await toggleStage('src/app.js');

    await waitInPage(
      () => {
        const row = Array.from(document.querySelectorAll('[role="row"]')).find((r) =>
          Array.from(r.querySelectorAll('[title]')).some((c) => c.getAttribute('title') === 'src/app.js')
        );
        return row?.querySelector('input[type="checkbox"]')?.checked === true;
      },
      [],
      { message: 'el archivo aparece como preparado' }
    );
    assert.equal(await fileStaged('src/app.js'), true, 'la casilla del archivo debería quedar marcada');

    const state = repoState(fx.repo);
    assert.ok(state.staged.includes('src/app.js'), `git no ve el archivo preparado: ${state.staged}`);
    assert.ok(!state.unstaged.includes('src/app.js'), 'el archivo no debería seguir en «sin preparar»');
    assert.deepEqual(state.staged.sort(), [fx.preparado, 'src/app.js'].sort(), 'solo esos dos archivos deben estar preparados');
  });
});
