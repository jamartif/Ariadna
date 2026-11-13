//! Repositorios sintéticos para E2E. Todo se crea con `git` real y se borra al final.

import { existsSync, mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import path from 'node:path';

import { git, tmpRoot } from './env.mjs';

/** Fechas fijas: ni la suite ni los baselines dependen de «ahora». */
const T0 = 1577836800; // 2020-01-01T00:00:00Z
const DAY = 86400;

function cdate(offsetDays) {
  return `${T0 + offsetDays * DAY} +0000`;
}

function commit(repo, subject, offsetDays) {
  const date = cdate(offsetDays);
  git(repo, ['add', '-A']);
  git(repo, ['commit', '-q', '--no-gpg-sign', '-m', subject], {
    GIT_AUTHOR_DATE: date,
    GIT_COMMITTER_DATE: date
  });
}

/**
 * Repositorio principal de la suite:
 *  - rama `main` con un commit inicial y `commitsTotales - 1` commits vacíos,
 *  - dos archivos modificados y uno nuevo sin preparar,
 *  - un archivo ya preparado.
 */
export function createRepoFixture({ name = 'repo-e2e', commitsTotales = 240 } = {}) {
  const root = tmpRoot();
  const repo = path.join(root, name);
  rmSync(repo, { recursive: true, force: true });
  mkdirSync(repo, { recursive: true });

  git(repo, ['init', '-q', '-b', 'main']);
  git(repo, ['config', 'user.name', 'E2E Ariadna']);
  git(repo, ['config', 'user.email', 'e2e@ariadna.test']);
  git(repo, ['config', 'commit.gpgsign', 'false']);
  git(repo, ['config', 'core.autocrlf', 'false']);
  git(repo, ['config', 'init.defaultBranch', 'main']);

  mkdirSync(path.join(repo, 'src'), { recursive: true });
  mkdirSync(path.join(repo, 'docs'), { recursive: true });
  writeFileSync(path.join(repo, 'README.md'), '# Proyecto E2E\n\nRepositorio sintético para las pruebas de Ariadna.\n');
  writeFileSync(
    path.join(repo, 'src', 'app.js'),
    ["export function saludar(nombre) {", "  return `Hola, ${nombre}`;", '}', ''].join('\n')
  );
  writeFileSync(path.join(repo, 'docs', 'guia.md'), '# Guía\n\nPasos para probar la aplicación.\n');
  commit(repo, 'Commit inicial del fixture', 0);

  for (let i = 2; i <= commitsTotales; i += 1) {
    const date = cdate(i);
    git(repo, ['commit', '-q', '--allow-empty', '--no-gpg-sign', '-m', `commit ${String(i).padStart(3, '0')} del historial`], {
      GIT_AUTHOR_DATE: date,
      GIT_COMMITTER_DATE: date
    });
  }

  // Cambios sin preparar (los dos primeros) y uno preparado.
  writeFileSync(
    path.join(repo, 'src', 'app.js'),
    ["export function saludar(nombre) {", "  return `Hola, ${nombre}!`;", '}', '', 'export const version = 2;', ''].join('\n')
  );
  writeFileSync(path.join(repo, 'docs', 'guia.md'), '# Guía\n\nPasos para probar la aplicación.\n\nNota añadida por la prueba.\n');
  writeFileSync(path.join(repo, 'nuevo.txt'), 'archivo nuevo sin preparar\n');
  writeFileSync(path.join(repo, 'preparado.txt'), 'ya está en el índice\n');
  git(repo, ['add', 'preparado.txt']);

  return {
    root,
    repo,
    commitsTotales,
    branchName: 'main',
    /** Archivos con cambios, en el orden en que los devuelve git status. */
    modificados: ['docs/guia.md', 'src/app.js'],
    nuevo: 'nuevo.txt',
    preparado: 'preparado.txt'
  };
}

/** Repositorio mínimo (un commit, sin cambios) para pantallas en limpio. */
export function createCleanRepo({ name = 'repo-limpio' } = {}) {
  const repo = path.join(tmpRoot(), name);
  rmSync(repo, { recursive: true, force: true });
  mkdirSync(repo, { recursive: true });
  git(repo, ['init', '-q', '-b', 'main']);
  git(repo, ['config', 'user.name', 'E2E Ariadna']);
  git(repo, ['config', 'user.email', 'e2e@ariadna.test']);
  writeFileSync(path.join(repo, 'README.md'), '# Limpio\n');
  commit(repo, 'Único commit', 0);
  return { repo };
}

/** Comprueba que el fixture sigue como espera la prueba (git, no la interfaz). */
export function repoState(repo) {
  const lines = (out) => out.split('\n').map((l) => l.trim()).filter(Boolean);
  return {
    branch: git(repo, ['rev-parse', '--abbrev-ref', 'HEAD']),
    head: git(repo, ['rev-parse', 'HEAD']),
    subject: git(repo, ['log', '-1', '--pretty=%s']),
    total: Number(git(repo, ['rev-list', '--count', 'HEAD'])),
    branches: lines(git(repo, ['for-each-ref', '--format=%(refname:short)', 'refs/heads'])),
    staged: lines(git(repo, ['diff', '--cached', '--name-only'])),
    unstaged: lines(git(repo, ['diff', '--name-only'])),
    untracked: lines(git(repo, ['ls-files', '--others', '--exclude-standard']))
  };
}

/** Limpia los temporales de la suite (mejor esfuerzo: la app puede seguir escribiendo). */
export function cleanupFixtures() {
  try {
    rmSync(tmpRoot(), { recursive: true, force: true, maxRetries: 5, retryDelay: 200 });
    return true;
  } catch {
    return false;
  }
}

export function readRepoFile(repo, rel) {
  return readFileSync(path.join(repo, rel), 'utf8');
}

export function fixtureExists(repo) {
  return existsSync(path.join(repo, '.git'));
}
