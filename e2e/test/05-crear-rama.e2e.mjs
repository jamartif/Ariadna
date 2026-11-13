//! Flujo 5 · crear una rama desde el panel Ramas.

import assert from 'node:assert/strict';

import { waitInPage } from '../lib/session.mjs';
import { showPanel } from '../lib/flows.mjs';
import { getFixture } from '../lib/state.mjs';
import { click, fill } from '../lib/ui.mjs';
import { repoState } from '../lib/fixture.mjs';

const BRANCH = 'feature/e2e-rama';

describe('Flujo 5 · crear rama', () => {
  it('crea la rama indicada y aparece en la lista', async () => {
    const fx = getFixture();
    await showPanel('Ramas');

    await click('Nueva rama');
    await fill('#branch-new-name', BRANCH);
    await click('Crear rama');

    await waitInPage(
      (name) => document.body.innerText.includes(name),
      [BRANCH],
      { message: 'la rama nueva aparece en la lista' }
    );

    const state = repoState(fx.repo);
    assert.ok(state.branches.includes(BRANCH), `git no tiene la rama: ${state.branches}`);
    assert.equal(state.branch, 'main', 'crear una rama no debe cambiar la rama actual');
    assert.ok(state.head.length >= 7, 'la rama nueva debe apuntar a un commit');
  });
});
