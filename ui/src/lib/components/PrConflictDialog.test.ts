import { render, screen, waitFor, within } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { mockAppliedConflicts } from '$lib/api/mockPr';
import { loadedTab, resetGlobals } from '../../test/helpers';
import PrConflictDialog from './PrConflictDialog.svelte';

beforeEach(() => {
  resetGlobals();
  mockAppliedConflicts.length = 0;
});

describe('Conflictos de una PR resueltos con IA', () => {
  it('propone alternativas, las enseña y no escribe nada hasta «Aplicar»', async () => {
    const { tab } = await loadedTab('tab-ariadna');
    // La PR #105 del mock es la que tiene conflictos de fusión.
    render(PrConflictDialog, { props: { tab, prNumber: 105, onclose: () => undefined } });

    const dlg = await screen.findByTestId('pr-conflicts-dialog');
    await within(dlg).findByTestId('pr-conflicts-file');
    // Antes de aplicar no hay nada escrito.
    expect(mockAppliedConflicts).toHaveLength(0);

    await userEvent.click(within(dlg).getByTestId('pr-conflicts-propose'));
    const options = await within(dlg).findAllByTestId('pr-conflicts-option');
    expect(options).toHaveLength(2);
    // La resolución se ve entera antes de elegir.
    expect(within(dlg).getByTestId('pr-conflicts-preview')).toHaveTextContent('hola');

    await userEvent.click(within(dlg).getByTestId('pr-conflicts-apply'));
    await waitFor(() => expect(within(dlg).getByTestId('pr-conflicts-done')).toBeInTheDocument());
    expect(mockAppliedConflicts).toEqual([
      { id: 105, files: ['ui/src/lib/api/types.ts'] }
    ]);
  });

  it('no deja resolver con IA un fichero recortado (se escribiría a medias)', async () => {
    const { tab } = await loadedTab('tab-ariadna');
    // Un fichero grande: su versión llega recortada, así que la IA no debe proponer nada.
    vi.spyOn(tab.api, 'prConflicts').mockResolvedValue({
      number: 105,
      base: 'main',
      head: 'mejoras',
      baseSha: 'a'.repeat(40),
      headSha: 'b'.repeat(40),
      files: [
        {
          path: 'assets/datos-enormes.txt',
          ours: 'linea\n'.repeat(10),
          theirs: 'otra\n'.repeat(10),
          base: 'base\n'.repeat(10),
          binary: false,
          truncated: true
        }
      ],
      truncated: false,
      tab: 'repo (main)'
    });
    render(PrConflictDialog, { props: { tab, prNumber: 105, onclose: () => undefined } });
    const dlg = await screen.findByTestId('pr-conflicts-dialog');
    const row = await within(dlg).findByTestId('pr-conflicts-file');
    expect(within(row).getByText('demasiado grande para la IA')).toBeInTheDocument();
    expect(within(row).getByTestId('pr-conflicts-propose')).toBeDisabled();
  });

  it('si se cancela la confirmación, no se aplica nada', async () => {
    const { tab } = await loadedTab('tab-ariadna', { confirm: () => false });
    render(PrConflictDialog, { props: { tab, prNumber: 105, onclose: () => undefined } });
    const dlg = await screen.findByTestId('pr-conflicts-dialog');
    await within(dlg).findByTestId('pr-conflicts-file');
    await userEvent.click(within(dlg).getByTestId('pr-conflicts-propose'));
    await within(dlg).findAllByTestId('pr-conflicts-option');
    await userEvent.click(within(dlg).getByTestId('pr-conflicts-apply'));
    await waitFor(() => expect(mockAppliedConflicts).toHaveLength(0));
    expect(within(dlg).queryByTestId('pr-conflicts-done')).toBeNull();
  });
});