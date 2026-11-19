//! Recorrer un diff largo: cabecera del hunk fija, salto al cambio anterior/siguiente y ajuste al
//! ancho sin arrastrar en horizontal.

import { cleanup, render, screen, waitFor } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import type { DiffHunk, DiffLine, FileDiff } from '$lib/api/types';
import { settings } from '$lib/stores/settings.svelte';
import { resetGlobals } from '../../test/helpers';
import CodeDiff from './CodeDiff.svelte';

beforeEach(() => {
  resetGlobals();
  settings.setDiffWrap(false);
  settings.setDiffLayout('unified');
});
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

const hunk = (id: string, start: number, lines: DiffLine[]): DiffHunk => ({
  id,
  oldStart: start,
  oldCount: lines.length,
  newStart: start,
  newCount: lines.length,
  heading: '',
  lines
});

/** Tres hunks separados: tres «cambios» por los que navegar. */
const varios: FileDiff = {
  path: 'src/app.ts',
  isBinary: false,
  isNewFile: false,
  isDeletedFile: false,
  hunks: [
    hunk('h1', 1, [line('a1', 'context', 'uno', 1, 1), line('a2', 'addition', 'dos', null, 2)]),
    hunk('h2', 40, [line('b1', 'context', 'tres', 40, 40), line('b2', 'deletion', 'cuatro', 41, null)]),
    hunk('h3', 90, [line('c1', 'context', 'cinco', 90, 90)])
  ]
} as FileDiff;

describe('CodeDiff · recorrer el diff', () => {
  it('la cabecera del hunk se queda fija mientras se recorre', async () => {
    render(CodeDiff, { props: { file: varios } });
    const sticky = await screen.findByTestId('diff-sticky-hunk');
    expect(sticky).toHaveTextContent('@@ -1,2 +1,2 @@');
    expect(sticky).toHaveAttribute('data-hunk', '1');
  });

  it('salta al cambio siguiente y al anterior', async () => {
    render(CodeDiff, { props: { file: varios } });
    const sticky = await screen.findByTestId('diff-sticky-hunk');
    expect(sticky).toHaveAttribute('data-hunk', '1');

    await userEvent.click(screen.getByTestId('diff-next-change'));
    await waitFor(() => expect(screen.getByTestId('diff-sticky-hunk')).toHaveAttribute('data-hunk', '2'));
    await userEvent.click(screen.getByTestId('diff-next-change'));
    await waitFor(() => expect(screen.getByTestId('diff-sticky-hunk')).toHaveAttribute('data-hunk', '3'));
    // En el último cambio, «siguiente» no sale del diff.
    await userEvent.click(screen.getByTestId('diff-next-change'));
    expect(screen.getByTestId('diff-sticky-hunk')).toHaveAttribute('data-hunk', '3');

    await userEvent.click(screen.getByTestId('diff-prev-change'));
    await waitFor(() => expect(screen.getByTestId('diff-sticky-hunk')).toHaveAttribute('data-hunk', '2'));
  });

  it('el ajuste al ancho es una opción que se recuerda y parte las líneas largas', async () => {
    const larga = 'x'.repeat(200);
    const conLarga: FileDiff = {
      path: 'src/larga.ts',
      isBinary: false,
      isNewFile: false,
      isDeletedFile: false,
      hunks: [hunk('h1', 1, [line('l1', 'addition', larga, null, 1)])]
    } as FileDiff;

    expect(settings.diffWrap).toBe(false);
    render(CodeDiff, { props: { file: conLarga } });
    await userEvent.click(screen.getByTestId('diff-wrap'));

    expect(settings.diffWrap).toBe(true);
    expect(localStorage.getItem('ariadna.diffWrap.v1')).toBe('1');
    // La línea larga ocupa varias filas de alto en vez de pedir scroll horizontal.
    const row = document.querySelector('.line') as HTMLElement;
    expect(Number(row.style.height.replace('px', ''))).toBeGreaterThan(settings.codeLineHeight);
    expect(document.querySelector('.code')?.classList.contains('wrapped')).toBe(true);
  });
});
