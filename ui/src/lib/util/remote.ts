/**
 * Reconocimiento del proveedor a partir de la URL (spec §4.3, `RemoteInfo::parse`).
 * Es una función pura y tolerante: nunca lanza con basura. Solo sirve para mostrar
 * icono y pistas; Git es quien decide si la URL vale.
 */

export type RemoteProvider = 'azure' | 'github' | 'gitlab' | 'bitbucket' | 'other';
export type RemoteScheme = 'https' | 'ssh' | 'unknown';

export interface RemoteGuess {
  provider: RemoteProvider;
  host: string;
  owner: string | null;
  project: string | null;
  repo: string;
  scheme: RemoteScheme;
  /** `https://org@dev.azure.com/…`: el usuario de la URL no es un secreto. */
  urlUser: string | null;
  /** La URL lleva `usuario:contraseña@`. Nunca se acepta: se pide en el formulario. */
  embeddedSecret: boolean;
}

function decode(s: string): string {
  try {
    return decodeURIComponent(s);
  } catch {
    return s;
  }
}

function stripGit(s: string): string {
  return s.replace(/\/+$/, '').replace(/\.git$/i, '');
}

function providerOf(host: string): RemoteProvider {
  const h = host.toLowerCase();
  if (h === 'dev.azure.com' || h === 'ssh.dev.azure.com' || h.endsWith('.visualstudio.com') || h === 'vs-ssh.visualstudio.com') {
    return 'azure';
  }
  if (h === 'github.com' || h.endsWith('.github.com') || h.startsWith('github.')) return 'github';
  if (h === 'gitlab.com' || h.endsWith('.gitlab.com') || h.startsWith('gitlab.')) return 'gitlab';
  if (h === 'bitbucket.org' || h.endsWith('.bitbucket.org')) return 'bitbucket';
  return 'other';
}

function segments(path: string): string[] {
  return path
    .split('/')
    .filter((s) => s !== '')
    .map(decode);
}

function build(
  host: string,
  path: string,
  scheme: RemoteScheme,
  urlUser: string | null,
  embeddedSecret: boolean
): RemoteGuess | null {
  const provider = providerOf(host);
  let segs = segments(stripGit(path));
  if (segs.length === 0) return null;
  let owner: string | null = null;
  let project: string | null = null;
  let repo = segs[segs.length - 1] as string;

  if (provider === 'azure') {
    const h = host.toLowerCase();
    if (h === 'ssh.dev.azure.com' || h === 'vs-ssh.visualstudio.com') {
      // git@ssh.dev.azure.com:v3/{org}/{project}/{repo}
      if (segs[0]?.toLowerCase() === 'v3') segs = segs.slice(1);
      if (segs.length >= 3) [owner, project] = [segs[0] as string, segs[1] as string];
      repo = segs[segs.length - 1] as string;
    } else if (h.endsWith('.visualstudio.com')) {
      owner = h.replace(/\.visualstudio\.com$/, '');
      const gi = segs.indexOf('_git');
      const pre = (gi >= 0 ? segs.slice(0, gi) : segs.slice(0, -1)).filter((s) => s.toLowerCase() !== 'defaultcollection');
      project = pre[0] ?? null;
      repo = gi >= 0 ? (segs[gi + 1] ?? repo) : repo;
    } else {
      const gi = segs.indexOf('_git');
      owner = segs[0] ?? null;
      if (gi > 1) project = segs[1] ?? null;
      repo = gi >= 0 ? (segs[gi + 1] ?? repo) : repo;
      // `https://dev.azure.com/org/_git/repo`: el proyecto lleva el nombre del repo.
    }
  } else if (segs.length >= 2) {
    owner = segs[0] as string;
    repo = segs[segs.length - 1] as string;
  }
  if (repo === '' || repo === '_git') return null;
  return { provider, host: host.toLowerCase(), owner, project, repo, scheme, urlUser, embeddedSecret };
}

export function parseRemote(input: string): RemoteGuess | null {
  const raw = input.trim();
  if (raw === '' || /\s/.test(raw.replace(/%20/g, ''))) {
    // Se admiten espacios solo si la URL es válida con ellos; el caso común no los lleva.
    if (raw === '') return null;
  }
  // ssh://[user@]host[:port]/ruta   o   https://[user[:pass]@]host/ruta
  const url = /^([a-z][a-z0-9+.-]*):\/\/(?:([^@/\s]*)@)?([^/:\s]+)(?::\d+)?(\/.*)?$/i.exec(raw);
  if (url) {
    const [, scheme, auth, host, path] = url as unknown as [string, string, string | undefined, string, string | undefined];
    const s = scheme.toLowerCase();
    if (s !== 'https' && s !== 'http' && s !== 'ssh' && s !== 'git') return null;
    const embeddedSecret = auth !== undefined && auth.includes(':') && auth.split(':').slice(1).join(':') !== '';
    const urlUser = auth ? (auth.split(':')[0] ?? null) || null : null;
    return build(host, path ?? '', s === 'ssh' || s === 'git' ? 'ssh' : 'https', urlUser, embeddedSecret);
  }
  // scp-like: [user@]host:ruta
  const scp = /^(?:([^@/\s]+)@)?([^:/\s]+):(?!\/\/)(.+)$/.exec(raw);
  if (scp) {
    const [, user, host, path] = scp as unknown as [string, string | undefined, string, string];
    return build(host, path, 'ssh', user ?? null, false);
  }
  return null;
}

export type CloneUrlProblem = 'empty' | 'invalid' | 'embedded-secret';

export function checkCloneUrl(input: string): CloneUrlProblem | null {
  if (input.trim() === '') return 'empty';
  const g = parseRemote(input);
  if (!g) return 'invalid';
  if (g.embeddedSecret) return 'embedded-secret';
  return null;
}

/** Nombre de carpeta sugerido: el del repositorio. */
export function suggestFolderName(input: string): string {
  const g = parseRemote(input);
  if (!g) return '';
  return g.repo.replace(/[<>:"/\\|?*\u0000-\u001f]/g, '-').replace(/^\.+/, '').trim();
}

/** Une una carpeta y un nombre con el separador que ya usa la carpeta. */
export function joinPath(parent: string, name: string): string {
  if (parent === '') return name;
  const sep = parent.includes('\\') && !parent.includes('/') ? '\\' : '/';
  return parent.replace(/[\\/]+$/, '') + sep + name;
}

export interface PatGuide {
  url: string;
  /** Clave i18n con los permisos mínimos. */
  perms: 'clone.perms.azure' | 'clone.perms.github' | 'clone.perms.gitlab' | 'clone.perms.bitbucket';
}

/** Enlaces de «cómo crear un PAT» (hosts permitidos por `open_external_url`). */
export function patGuide(provider: RemoteProvider): PatGuide | null {
  switch (provider) {
    case 'azure':
      return {
        url: 'https://learn.microsoft.com/azure/devops/organizations/accounts/use-personal-access-tokens-to-authenticate',
        perms: 'clone.perms.azure'
      };
    case 'github':
      return {
        url: 'https://docs.github.com/authentication/keeping-your-account-and-data-secure/managing-your-personal-access-tokens',
        perms: 'clone.perms.github'
      };
    case 'gitlab':
      return { url: 'https://docs.gitlab.com/user/profile/personal_access_tokens/', perms: 'clone.perms.gitlab' };
    case 'bitbucket':
      return {
        url: 'https://support.atlassian.com/bitbucket-cloud/docs/repository-access-tokens/',
        perms: 'clone.perms.bitbucket'
      };
    default:
      return null;
  }
}
