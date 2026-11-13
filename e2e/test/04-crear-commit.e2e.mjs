//! Flujo 4 · crear un commit desde la barra de commit.

import assert from 'node:assert/strict';

import { getBrowser, waitInPage } from '../lib/session.mjs';
import { showView } from '../lib/flows.mjs';
import { getFixture } from '../lib/state.mjs';
import { clickTestId, fileSection, fill } from '../lib/ui.mjs';
import { repoState } from '../lib/fixture.mjs';

const SUBJECT = 'feat(e2e): commit creado por la suite';

describe('Flujo 4 · crear commit', () => {
  it('confirma el commit con el asunto escrito y el archivo preparado', async () => {
    const fx = getFixture();
    await showView('changes');
    const before = repoState(fx.repo);

    // El campo del asunto tiene id estable (`commit-summary-<pestaña>`).
    await fill('input[id^="commit-summary-"]', SUBJECT);
    await clickTestId('commit-submit');

    // git es la prueba de verdad; la interfaz se comprueba después.
    await getBrowser().waitUntil(async () => repoState(fx.repo).subject === SUBJECT, {
      timeout: 30_000,
      interval: 250,
      timeoutMsg: 'git no registró el commit'
    });

    const after = repoState(fx.repo);
    assert.equal(after.subject, SUBJECT, 'git debería tener el asunto del commit');
    assert.equal(after.total, before.total + 1, 'el número de commits debería aumentar en uno');
    assert.ok(!after.staged.includes('src/app.js'), 'el archivo preparado debería haberse confirmado');
    assert.notEqual(after.head, before.head, 'HEAD debería apuntar a un commit nuevo');

    // La interfaz refresca el estado: el archivo confirmado ya no aparece en la lista.
    await getBrowser().waitUntil(async () => (await fileSection('src/app.js')) === null, {
      timeout: 20_000,
      interval: 250,
      timeoutMsg: 'la lista de cambios no se actualizó tras el commit'
    });
    assert.equal(await fileSection('docs/guia.md'), 'unstaged', 'los cambios restantes deberían seguir en la lista');

    // El historial (backend) contiene el commit nuevo.
    await showView('history');
    await waitInPage(
      (subject) => document.body.innerText.includes(subject),
      [SUBJECT],
      { message: 'el commit nuevo aparece en el historial' }
    );
    await showView('changes');
  });
});
