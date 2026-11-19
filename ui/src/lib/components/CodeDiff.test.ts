//! `CodeDiff` (historial y PR): la vista partida se ofrece aquí también, menos en las PR,
//! donde los comentarios se anclan a una línea concreta.

import { cleanup, render, screen, waitFor } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import type { DiffHunk, DiffLine, FileDiff } from '$lib/api/types';
import { resetGlobals } from '../../test/helpers';
import CodeDiff from './CodeDiff.svelte';

beforeEach(resetGlobals);
afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
  resetGlobals();
});

const line = (id: string, kind: DiffLine['kind'], text: string, o: number | null, n: number | null): DiffLine => ({
  id,
  kind,
  text,
  oldLineNumber: o,
  newLineNumber: n
});

const file: FileDiff = {
  path: 'src/app.js',
  isBinary: false,
  isNewFile: false,
  isDeletedFile: false,
  hunks: [
    {
      id: 'h1',
      oldStart: 1,
      oldCount: 2,
      newStart: 1,
      newCount: 2,
      heading: '',
      lines: [line('a', 'context', 'const a = 1;', 1, 1), line('b', 'deletion', 'viejo', 2, null), line('c', 'addition', 'nuevo', null, 2)]
    }
  ]
} as FileDiff;

describe('CodeDiff · vista partida', () => {
  it('con un commit se puede ver en dos columnas y el contenido queda en las dos', async () => {
    render(CodeDiff, { props: { file } });
    // La cabecera sale dos veces: la fila del hunk y la barra fija de arriba.
    expect(screen.getAllByText(/^@@ -1,2 \+1,2 @@$/).length).toBeGreaterThan(0);
    expect(screen.getByTestId('diff-sticky-hunk')).toHaveTextContent('@@ -1,2 +1,2 @@');

    await userEvent.click(screen.getByRole('button', { name: /Dos columnas/ }));
    await waitFor(() => expect(document.querySelectorAll('.srow').length).toBeGreaterThan(0));
    // Lo borrado a la izquierda, lo añadido a la derecha.
    expect(screen.getByText('viejo')).toBeInTheDocument();
    expect(screen.getByText('nuevo')).toBeInTheDocument();

    await userEvent.click(screen.getByRole('button', { name: /Una columna/ }));
    await waitFor(() => expect(document.querySelectorAll('.srow').length).toBe(0));
  });

  it('en una pull request no se ofrece la vista partida (los comentarios van por línea)', () => {
    render(CodeDiff, { props: { file, onlineaction: () => undefined } });
    expect(screen.queryByRole('button', { name: /Dos columnas/ })).toBeNull();
  });
});
