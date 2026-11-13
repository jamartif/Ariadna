//! Flujo 6 · cambiar de rama (con aviso por cambios sin confirmar).

import assert from 'node:assert/strict';

import { getBrowser, waitInPage } from '../lib/session.mjs';
import { showPanel } from '../lib/flows.mjs';
import { getFixture } from '../lib/state.mjs';
import { click, hasText } from '../lib/ui.mjs';
import { repoState } from '../lib/fixture.mjs';

const BRANCH = 'feature/e2e-rama';

describe('Flujo 6 · cambiar de rama', () => {
  it('cambia a la rama creada y confirma el aviso de cambios sin preparar', async () => {
    const fx = getFixture();
    await showPanel('Ramas');

    // Selección de la rama en la lista (la fila se marca como seleccionada).
    await click(BRANCH);
    await waitInPage(
      (name) =>
        Array.from(document.querySelectorAll('[role="button"], button')).some(
          (el) => el.getAttribute('aria-pressed') === 'true' && (el.textContent ?? '').includes(name)
        ),
      [BRANCH],
      { message: 'la rama nueva queda seleccionada' }
    );

    await click('Cambiar a esta rama');

    // Hay cambios sin confirmar (docs/guia.md, nuevo.txt): la app pide confirmación.
    await waitInPage(() => document.body.innerText.includes('Cambiar de todos modos'), [], {
      message: 'aviso de cambios sin confirmar'
    });
    await click('Cambiar de todos modos');

    await waitInPage(
      (name) => document.body.innerText.includes(`Ahora estás en «${name}»`) || document.body.innerText.includes(name),
      [BRANCH],
      { message: 'aviso de rama cambiada' }
    );

    // Espera a que git confirme el cambio (evita leer un estado intermedio).
    await getBrowser().waitUntil(async () => repoState(fx.repo).branch === BRANCH, {
      timeout: 15_000,
      interval: 250,
      timeoutMsg: 'git no cambió de rama'
    });

    const state = repoState(fx.repo);
    assert.equal(state.branch, BRANCH);
    // Los cambios se llevan consigo: siguen sin confirmar en la rama nueva.
    assert.ok(state.unstaged.includes('docs/guia.md'), 'los cambios deberían viajar a la rama nueva');
    assert.ok(await hasText(BRANCH), 'la interfaz debería seguir mostrando la rama actual');
    assert.match(
      await getBrowser().execute(() =>
        Array.from(document.querySelectorAll('[role="tab"][aria-label]'))
          .map((el) => el.getAttribute('aria-label'))
          .join(' | ')
      ),
      /feature\/e2e-rama/,
      'la pestaña del repositorio debería indicar la rama nueva'
    );
  });
});
