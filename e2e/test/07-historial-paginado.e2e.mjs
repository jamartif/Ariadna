//! Flujo 7 · historial paginado (100 por página, carga al llegar al final).

import assert from 'node:assert/strict';

import { waitInPage } from '../lib/session.mjs';
import { showPanel } from '../lib/flows.mjs';
import { getFixture } from '../lib/state.mjs';
import { historyCountText, scrollHistoryToBottom } from '../lib/ui.mjs';
import { repoState } from '../lib/fixture.mjs';

const num = (text) => Number((text.match(/\d+/g) ?? ['0'])[0]);

describe('Flujo 7 · historial paginado', () => {
  it('carga la primera página y va pidiendo más al desplazarse', async () => {
    const fx = getFixture();
    await showPanel('Historial');

    await waitInPage(() => /\d+ commits? cargados/.test(document.body.innerText), [], {
      message: 'contador del historial'
    });

    const first = num(await historyCountText());
    assert.ok(first > 0 && first <= 100, `la primera página debería traer hasta 100 commits, trajo ${first}`);

    const total = repoState(fx.repo).total;
    assert.ok(total > 100, `el fixture debería tener más de 100 commits, tiene ${total}`);

    const counts = [first];
    for (let i = 0; i < 6 && counts.at(-1) < total; i += 1) {
      const before = counts.at(-1);
      await scrollHistoryToBottom();
      await waitInPage(
        (n) => {
          const el = document.querySelector('[aria-live="polite"]');
          const m = (el?.textContent ?? '').match(/\d+/);
          return m ? Number(m[0]) > n : false;
        },
        [before],
        { message: 'siguiente página del historial', timeout: 20_000 }
      );
      counts.push(num(await historyCountText()));
    }

    assert.ok(counts.length > 1, `la paginación no avanzó: ${counts.join(' → ')}`);
    assert.equal(counts.at(-1), total, `debería haberse cargado el historial completo (${counts.join(' → ')})`);

    // El commit más antiguo del fixture ya se puede ver en la lista.
    await scrollHistoryToBottom();
    await waitInPage(() => document.body.innerText.includes('Commit inicial del fixture'), [], {
      message: 'el commit más antiguo del historial'
    });
  });
});
