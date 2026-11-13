//! Flujo 6 · cambiar de rama desde el riel (con filtro y aviso por cambios sin confirmar).

import assert from 'node:assert/strict';

import { getBrowser } from '../lib/session.mjs';
import {
  checkoutBranch,
  currentBranchName,
  ensureRailOpen,
  filterBranches,
  selectBranch,
  showView
} from '../lib/flows.mjs';
import { getFixture } from '../lib/state.mjs';
import { clickTestId, tabs } from '../lib/ui.mjs';
import { repoState } from '../lib/fixture.mjs';

const BRANCH = 'feature/e2e-rama';

describe('Flujo 6 · cambiar de rama', () => {
  it('filtra la rama en el riel, la selecciona y confirma el aviso de cambios sin preparar', async () => {
    const fx = getFixture();
    await showView('changes');
    await ensureRailOpen();

    // El filtro del riel es instantáneo: deja la rama buscada a la vista.
    await filterBranches('e2e-rama');
    await selectBranch(BRANCH);

    // El inspector de la rama seleccionada ofrece «cambiar» y el detalle en el panel derecho.
    assert.ok(
      await getBrowser().execute(() => !!document.querySelector('[data-testid="rail-inspector"]')),
      'el inspector de la rama debería estar abierto'
    );

    // Hay cambios sin confirmar (docs/guia.md, nuevo.txt): la app pide confirmación
    // y la prueba acepta «cambiar igualmente» (lo hace `checkoutBranch`).
    await clickTestId('rail-clear-filter', { timeout: 2000 }).catch(() => {});
    await checkoutBranch(BRANCH);

    // git confirma el cambio (se espera a que el estado del repositorio lo refleje).
    await getBrowser().waitUntil(async () => repoState(fx.repo).branch === BRANCH, {
      timeout: 15_000,
      interval: 250,
      timeoutMsg: 'git no cambió de rama'
    });

    const state = repoState(fx.repo);
    assert.equal(state.branch, BRANCH);
    // Los cambios se llevan consigo: siguen sin confirmar en la rama nueva.
    assert.ok(state.unstaged.includes('docs/guia.md'), 'los cambios deberían viajar a la rama nueva');
    assert.equal(await currentBranchName(), BRANCH, 'la tarjeta del riel debería mostrar la rama nueva');

    const repoTab = (await tabs()).find((t) => t.kind === 'repo');
    assert.ok(repoTab, 'debería seguir habiendo una pestaña del repositorio');
    assert.match(repoTab.title, /feature\/e2e-rama/, 'la pestaña del repositorio debería indicar la rama nueva');
    assert.match(repoTab.text, /feature\/e2e-rama/, 'el texto de la pestaña debería indicar la rama nueva');
  });
});
