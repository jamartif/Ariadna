//! Flujo 4 · crear un commit desde la barra de commit.

import assert from 'node:assert/strict';

import { getBrowser, waitInPage } from '../lib/session.mjs';
import { showPanel } from '../lib/flows.mjs';
import { getFixture } from '../lib/state.mjs';
import { click, fill, hasText } from '../lib/ui.mjs';
import { repoState } from '../lib/fixture.mjs';

const SUBJECT = 'feat(e2e): commit creado por la suite';

describe('Flujo 4 · crear commit', () => {
  it('confirma el commit con el asunto escrito y el archivo preparado', async () => {
    const fx = getFixture();
    await showPanel('Cambios');
    const before = repoState(fx.repo);

    await fill('input[placeholder="Asunto del commit"]', SUBJECT);
    await click('Crear commit');

    await waitInPage(() => document.body.innerText.includes('Commit creado'), [], {
      message: 'aviso de commit creado'
    });

    const after = repoState(fx.repo);
    assert.equal(after.subject, SUBJECT, 'git debería tener el asunto del commit');
    assert.equal(after.total, before.total + 1, 'el número de commits debería aumentar en uno');
    assert.ok(!after.staged.includes('src/app.js'), 'el archivo preparado debería haberse confirmado');
    assert.notEqual(after.head, before.head, 'HEAD debería apuntar a un commit nuevo');

    // La interfaz refresca el estado: el archivo confirmado ya no aparece en la lista.
    await waitInPage(
      () =>
        !Array.from(document.querySelectorAll('[role="row"]')).some((row) =>
          (row.textContent ?? '').includes('src/app.js')
        ),
      [],
      { message: 'la lista de cambios se actualiza tras el commit' }
    );
    assert.ok(await hasText('docs/guia.md'), 'los cambios restantes deberían seguir en la lista');

    // El historial (backend) contiene el commit nuevo.
    await showPanel('Historial');
    await waitInPage(
      (subject) => document.body.innerText.includes(subject),
      [SUBJECT],
      { message: 'el commit nuevo aparece en el historial' }
    );
    await showPanel('Cambios');
  });
});
