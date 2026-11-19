import { afterEach, describe, expect, it } from 'vitest';
import type { AmendRefusal } from '$lib/api/types';
import { setLocale } from '$lib/i18n';
import { AMEND_REASON_KEYS, amendRefusalText } from './amendReason';

const motivos: AmendRefusal[] = [
  { kind: 'no_commits' },
  { kind: 'detached_head' },
  { kind: 'integration_in_progress' },
  { kind: 'already_published', value: 'origin/main' }
];

afterEach(() => setLocale('es'));

describe('motivos tipados del amend', () => {
  it('cada motivo se traduce en los dos idiomas y no devuelve la clave', () => {
    for (const locale of ['es', 'en'] as const) {
      setLocale(locale);
      for (const motivo of motivos) {
        const texto = amendRefusalText(motivo);
        const donde = `${locale}/${motivo.kind}`;
        expect(texto, donde).toBeTruthy();
        expect(texto, donde).not.toBe(AMEND_REASON_KEYS[motivo.kind]);
        expect(texto, donde).not.toContain('amendReason.');
      }
    }
  });

  it('los dos idiomas no comparten la redacción (no hay texto hardcodeado)', () => {
    for (const motivo of motivos) {
      setLocale('es');
      const es = amendRefusalText(motivo);
      setLocale('en');
      const en = amendRefusalText(motivo);
      expect(es, motivo.kind).not.toBe(en);
    }
  });

  it('el motivo «ya publicado» lleva el remoto que da el motor', () => {
    setLocale('es');
    expect(amendRefusalText({ kind: 'already_published', value: 'origin/main' })).toContain(
      'origin/main'
    );
    setLocale('en');
    const en = amendRefusalText({ kind: 'already_published', value: 'upstream/feature' });
    expect(en).toContain('upstream/feature');
  });

  it('sin motivo no hay texto que pintar', () => {
    expect(amendRefusalText(null)).toBeNull();
    expect(amendRefusalText(undefined)).toBeNull();
  });
});
