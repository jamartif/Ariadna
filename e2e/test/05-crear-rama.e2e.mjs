//! Flujo 5 · crear una rama desde el riel de ramas.

import assert from 'node:assert/strict';

import { waitInPage } from '../lib/session.mjs';
import { createBranch, currentBranchName, ensureRepoOpen, ensureRailOpen } from '../lib/flows.mjs';
import { getFixture } from '../lib/state.mjs';
import { hasTestId } from '../lib/ui.mjs';
import { repoState } from '../lib/fixture.mjs';

const BRANCH = 'feature/e2e-rama';

describe('Flujo 5 · crear rama', () => {
  it('crea la rama indicada y aparece en el riel sin cambiar la rama actual', async () => {
    const fx = getFixture();
    await ensureRepoOpen();
    await ensureRailOpen();
    assert.equal(await currentBranchName(), 'main', 'el fixture debería estar en main');

    await createBranch(BRANCH);

    // El riel la lista desde el estado nuevo del repositorio.
    await waitInPage(
      (name) => Array.from(document.querySelectorAll('[data-brail-name]')).some((el) => el.dataset.brailName === name),
      [BRANCH],
      { message: 'la rama nueva aparece en el riel' }
    );

    const state = repoState(fx.repo);
    assert.ok(state.branches.includes(BRANCH), `git no tiene la rama: ${state.branches}`);
    assert.equal(state.branch, 'main', 'crear una rama no debe cambiar la rama actual');
    assert.ok(state.head.length >= 7, 'la rama nueva debe apuntar a un commit');
    assert.equal(await currentBranchName(), 'main', 'la tarjeta de rama actual no debería cambiar');
    assert.equal(await hasTestId('rail-create'), false, 'el formulario de creación debería cerrarse');
  });
});
