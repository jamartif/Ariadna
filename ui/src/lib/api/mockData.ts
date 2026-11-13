import type { Commit, DiffHunk, DiffLine } from './types';

/** Fila de especificación de un hunk: ` ` contexto, `+` adición, `-` borrado. */
export type LineSpec = string;

/**
 * Construye un hunk calculando números de línea y recuentos a partir de las
 * líneas con prefijo (' ', '+', '-').
 */
export function makeHunk(
  id: string,
  oldStart: number,
  newStart: number,
  heading: string,
  specs: LineSpec[]
): DiffHunk {
  let o = oldStart;
  let n = newStart;
  let oldCount = 0;
  let newCount = 0;
  const lines: DiffLine[] = specs.map((s, i) => {
    const mark = s[0];
    const text = s.slice(1);
    const lineId = `${id}:${i}`;
    if (mark === '+') {
      newCount++;
      return { id: lineId, kind: 'addition', text, oldLineNumber: null, newLineNumber: n++ };
    }
    if (mark === '-') {
      oldCount++;
      return { id: lineId, kind: 'deletion', text, oldLineNumber: o++, newLineNumber: null };
    }
    oldCount++;
    newCount++;
    return { id: lineId, kind: 'context', text, oldLineNumber: o++, newLineNumber: n++ };
  });
  return { id, oldStart, oldCount, newStart, newCount, heading, lines };
}

/** Hunk de archivo nuevo: todo adiciones. */
export function additionsHunk(id: string, lines: string[]): DiffHunk {
  return makeHunk(id, 0, 1, '', lines.map((l) => '+' + l));
}

/** Hunk de archivo borrado: todo borrados. */
export function deletionsHunk(id: string, lines: string[]): DiffHunk {
  const h = makeHunk(id, 1, 0, '', lines.map((l) => '-' + l));
  return h;
}

export function sha(seed: number): string {
  let x = seed * 2654435761;
  let s = '';
  for (let i = 0; i < 40; i++) {
    x = (x * 1103515245 + 12345) & 0x7fffffff;
    s += (x % 16).toString(16);
  }
  return s;
}

export function makeCommits(subjects: string[], baseDate = '2026-11-06T10:00:00+01:00'): Commit[] {
  const base = new Date(baseDate).getTime();
  return subjects.map((subject, i) => ({
    sha: sha(i + 7),
    parents: i + 1 < subjects.length ? [sha(i + 8)] : [],
    subject,
    body: '',
    authorName: 'Javier',
    authorEmail: 'javier@example.com',
    authorDateIso: new Date(base - i * 86_400_000 * 1.3).toISOString(),
    refs: i === 0 ? ['HEAD -> main', 'origin/main'] : []
  }));
}

// ---- Contenido de ejemplo --------------------------------------------------------

export const TYPES_TS_UNSTAGED: DiffHunk[] = [
  makeHunk('types.ts#u1', 14, 14, 'export type GitPath', [
    ' ',
    ' export type GitPath = string | number[];',
    ' ',
    '-export type ChangeStatus = "." | "M" | "A" | "D";',
    "+export type ChangeStatus = '.' | 'M' | 'A' | 'D' | 'R' | 'C' | 'U' | 'T' | '?' | '!' | '';",
    ' ',
    ' export type ConflictKind ='
  ]),
  makeHunk('types.ts#u2', 88, 88, 'export interface Snapshot', [
    '   branches: Branch[];',
    '   remoteBranches: Branch[];',
    '   remotes: Remote[];',
    '+  integration: IntegrationState;',
    '+  conflicts: ConflictFile[];',
    ' }',
    ' '
  ])
];

export const TYPES_TS_STAGED: DiffHunk[] = [
  makeHunk('types.ts#s1', 1, 1, '', [
    ' /**',
    '- * Tipos de la interfaz.',
    '+ * Tipos de la interfaz, espejo de crates/ariadna-git/src/models.rs',
    '+ * serializados con serde en camelCase.',
    '  */'
  ])
];

export const MODELS_RS_STAGED: DiffHunk[] = [
  makeHunk('models.rs#s1', 212, 212, 'pub struct GitStatus', [
    ' pub struct GitStatus {',
    '     pub branch_name: Option<String>,',
    '     pub head_oid: Option<String>,',
    '+    pub is_unborn: bool,',
    '     pub is_detached: bool,',
    ' '
  ])
];

export const IPC_MD_UNSTAGED: DiffHunk[] = [
  makeHunk('ipc.md#u1', 20, 20, '## Comandos de aplicación', [
    ' | `app_state` | — | `AppState` |',
    '-| `pick_folder` | — | `string` |',
    '+| `pick_folder` | — | `string \\| null` (diálogo nativo) |',
    ' | `open_repository` | `path` | `OpenResult` |'
  ])
];

export const APP_SVELTE_ADDED: DiffHunk = additionsHunk('App.svelte#s1', [
  '<script lang="ts">',
  "  import TitleBar from '$lib/components/TitleBar.svelte';",
  "  import TabStrip from '$lib/components/TabStrip.svelte';",
  '',
  '  let { app } = $props();',
  '</script>',
  '',
  '<TitleBar />',
  '<TabStrip entries={app.entries} />'
]);

export const LEGACY_SWIFT_DELETED: DiffHunk = deletionsHunk('Legacy.swift#u1', [
  'import Foundation',
  '',
  '/// Capa de compatibilidad con la v0.4. Se elimina en la migración a Rust.',
  'enum LegacyBridge {',
  '    static func migrate() {',
  '        // …',
  '    }',
  '}'
]);

export const README_RENAMED: DiffHunk[] = [
  makeHunk('README.md#s1', 1, 1, '', [
    '-# Ariadna (Swift)',
    '+# Ariadna',
    ' ',
    ' Cliente Git de escritorio con revisión de cambios y mensajes de commit asistidos.'
  ])
];

export const NOTAS_UNTRACKED: DiffHunk = additionsHunk('notas.txt#u1', [
  'Pendiente de la oleada 2:',
  '- historial virtualizado',
  '- resolución de conflictos',
  '- ramas y remotos'
]);

export const ERROR_DEMO_UNTRACKED: DiffHunk = additionsHunk('error-demo.md#u1', [
  '# Ejemplo de error',
  'Preparar este archivo falla con LOCKED (index.lock) en el backend simulado.'
]);

export const MAKEFILE_UNSTAGED: DiffHunk[] = [
  makeHunk('Makefile#u1', 3, 3, 'build:', [
    ' build:',
    ' \tcargo build --release',
    '-\tcd ui && npm run build',
    '+\tcd ui && npm ci && npm run build',
    '+\t@echo "listo"',
    ' '
  ])
];

export function longLineHunk(): DiffHunk {
  const long = 'const data = [' + Array.from({ length: 400 }, (_, i) => `"item-${i}"`).join(', ') + '];';
  return makeHunk('minified.js#u1', 1, 1, '', [
    '-var a=1;',
    '+var a=2;',
    '+' + long,
    ' module.exports = a;'
  ]);
}

/** Hunk gigante (6 200 líneas) para probar el colapso de NF-5. */
export function giantHunk(): DiffHunk {
  const lines = Array.from({ length: 6200 }, (_, i) => `+  { "id": ${i}, "name": "elemento-${i}", "ok": ${i % 2 === 0} },`);
  return makeHunk('big-fixture.json#u1', 0, 1, '', lines);
}

export function smallHunk(path: string, i: number): DiffHunk {
  return makeHunk(`${path}#u1`, 10, 10, 'fn main', [
    ' fn main() {',
    `-    println!("v${i}");`,
    `+    println!("v${i + 1}");`,
    ' }'
  ]);
}
