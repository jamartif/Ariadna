/**
 * Datos de conflictos del backend simulado: archivos en conflicto con sus tres lados y el
 * archivo de trabajo con marcadores (estilo diff3), para el estudio de conflictos.
 */
import { parseMarkers } from '$lib/util/conflictMarkers';
import type { ConflictFile, ConflictKind, ConflictSummary, GitPath } from './types';

export interface MockConflictDef {
  kind: ConflictKind;
  ours: string | null;
  base: string | null;
  theirs: string | null;
  binary?: boolean;
  working: string | null;
}

const INVOICE_BASE = `import { Money } from './money';

export interface Invoice {
  id: string;
  lines: Line[];
}

export function total(invoice: Invoice): Money {
  return invoice.lines.reduce((acc, l) => acc.plus(l.amount), Money.zero());
}

export function format(invoice: Invoice): string {
  return \`Factura \${invoice.id}\`;
}
`;

const INVOICE_OURS = `import { Money } from './money';
import { applyDiscount } from './discount';

export interface Invoice {
  id: string;
  lines: Line[];
}

export function total(invoice: Invoice): Money {
  const gross = invoice.lines.reduce((acc, l) => acc.plus(l.amount), Money.zero());
  return applyDiscount(gross, invoice);
}

export function format(invoice: Invoice): string {
  return \`Factura nº \${invoice.id}\`;
}
`;

const INVOICE_THEIRS = `import { Money } from './money';
import { taxFor } from './tax-rates';

export interface Invoice {
  id: string;
  currency: string;
  lines: Line[];
}

export function total(invoice: Invoice): Money {
  const net = invoice.lines.reduce((acc, l) => acc.plus(l.amount), Money.zero());
  return net.plus(taxFor(net, invoice.currency));
}

export function format(invoice: Invoice): string {
  return \`Factura \${invoice.id} (\${invoice.currency})\`;
}
`;

const INVOICE_WORKING = `import { Money } from './money';
<<<<<<< HEAD
import { applyDiscount } from './discount';
||||||| merged common ancestors
=======
import { taxFor } from './tax-rates';
>>>>>>> origin/main

export interface Invoice {
  id: string;
<<<<<<< HEAD
||||||| merged common ancestors
=======
  currency: string;
>>>>>>> origin/main
  lines: Line[];
}

export function total(invoice: Invoice): Money {
<<<<<<< HEAD
  const gross = invoice.lines.reduce((acc, l) => acc.plus(l.amount), Money.zero());
  return applyDiscount(gross, invoice);
||||||| merged common ancestors
  return invoice.lines.reduce((acc, l) => acc.plus(l.amount), Money.zero());
=======
  const net = invoice.lines.reduce((acc, l) => acc.plus(l.amount), Money.zero());
  return net.plus(taxFor(net, invoice.currency));
>>>>>>> origin/main
}

export function format(invoice: Invoice): string {
<<<<<<< HEAD
  return \`Factura nº \${invoice.id}\`;
||||||| merged common ancestors
  return \`Factura \${invoice.id}\`;
=======
  return \`Factura \${invoice.id} (\${invoice.currency})\`;
>>>>>>> origin/main
}
`;

const TAX_BASE = `export const IVA = 0.21;

export function iva(amount: number): number {
  return amount * IVA;
}
`;

const TAX_OURS = `export const IVA = 0.21;

export function iva(amount: number): number {
  return Math.round(amount * IVA * 100) / 100;
}
`;

const SETTINGS_OURS = `{
  "currency": "EUR",
  "locale": "es-ES"
}
`;

const SETTINGS_THEIRS = `{
  "currency": "USD",
  "retries": 3
}
`;

const SETTINGS_WORKING = `{
<<<<<<< HEAD
  "currency": "EUR",
  "locale": "es-ES"
=======
  "currency": "USD",
  "retries": 3
>>>>>>> origin/main
}
`;

export function demoConflictDefs(): Map<string, MockConflictDef> {
  return new Map<string, MockConflictDef>([
    ['src/billing/invoice.ts', { kind: 'bothModified', ours: INVOICE_OURS, base: INVOICE_BASE, theirs: INVOICE_THEIRS, working: INVOICE_WORKING }],
    ['src/billing/tax.ts', { kind: 'deletedByThem', ours: TAX_OURS, base: TAX_BASE, theirs: null, working: TAX_OURS }],
    ['config/settings.json', { kind: 'bothAdded', ours: SETTINGS_OURS, base: null, theirs: SETTINGS_THEIRS, working: SETTINGS_WORKING }],
    ['assets/logo.png', { kind: 'bothModified', ours: '\u0000PNG ours', base: '\u0000PNG base', theirs: '\u0000PNG theirs', binary: true, working: null }]
  ]);
}

const side = (content: string | null, binary: boolean, n: number) => ({
  oid: content === null ? null : `${n}`.repeat(40).slice(0, 40),
  content: binary ? null : content,
  isBinary: binary
});

export function toConflictFile(path: string, d: MockConflictDef): ConflictFile {
  const binary = d.binary === true;
  return {
    path,
    kind: d.kind,
    ours: side(d.ours, binary, 2),
    base: side(d.base, binary, 1),
    theirs: side(d.theirs, binary, 3),
    workingTree: binary ? null : d.working,
    hunks: binary || d.working === null ? [] : parseMarkers(d.working).regions.map((r) => ({ id: r.id, ours: r.ours, base: r.base, theirs: r.theirs }))
  };
}

export function toSummary(path: GitPath, d: MockConflictDef): ConflictSummary {
  const f = toConflictFile(path as string, d);
  return {
    path,
    kind: d.kind,
    binary: d.binary === true,
    hunkCount: f.hunks.length,
    textSafe: d.binary !== true,
    oursPresent: d.ours !== null,
    basePresent: d.base !== null,
    theirsPresent: d.theirs !== null
  };
}
