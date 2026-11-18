import { render, screen, waitFor, within } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it } from 'vitest';
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