/**
 * Validadores estrictos de las formas de pull request (`prTypes.ts`). Los usan el test de
 * contrato contra los fixtures reales de Rust (`fixtures-pr/`) y contra `mockApi`.
 */

import { arr, bool, fileDiff, int, nullable, obj, oneOf, str, type Errors } from './validate';

type V = (v: unknown, path: string, errs: Errors) => void;

const provider = oneOf('azureDevOps', 'github', 'gitlab', 'bitbucket', 'other');

export const prUser = obj({ id: str, displayName: str, login: str });

export const prVote = oneOf('approve', 'approveWithSuggestions', 'noVote', 'waitForAuthor', 'reject', 'comment');
export const prStrategy = oneOf('merge', 'squash', 'rebase', 'rebaseMerge');

export const prReviewer = obj({ user: prUser, vote: prVote, required: bool });

export const prPolicyCheck = obj({
  name: str,
  status: oneOf('passing', 'failing', 'pending', 'notApplicable', 'broken'),
  blocking: nullable(bool),
  url: nullable(str)
});

export const prChecksSummary = obj({ passing: int, failing: int, pending: int });

const summaryShape: Record<string, V> = {
  id: int,
  title: str,
  author: prUser,
  sourceBranch: str,
  targetBranch: str,
  state: oneOf('active', 'completed', 'abandoned'),
  isDraft: bool,
  createdAt: str,
  updatedAt: nullable(str),
  reviewers: arr(prReviewer),
  checks: nullable(prChecksSummary),
  mergeStatus: oneOf('clean', 'conflicts', 'unknown'),
  webUrl: str
};

export const pullRequestSummary = obj(summaryShape);

export const prFileChange = obj({
  path: str,
  oldPath: nullable(str),
  status: oneOf('added', 'modified', 'deleted', 'renamed', 'copied', 'other'),
  additions: nullable(int),
  deletions: nullable(int),
  binary: bool
});

export const pullRequestDetail = obj({
  ...summaryShape,
  description: str,
  policies: arr(prPolicyCheck),
  workItems: arr(obj({ id: int, url: nullable(str) })),
  commits: arr(obj({ sha: str, subject: str, author: str, date: nullable(str) })),
  files: arr(prFileChange),
  sourceCommit: nullable(str),
  targetCommit: nullable(str),
  checkoutRef: str,
  isFork: bool,
  autoComplete: bool,
  mergeDetail: nullable(str)
});

export const pullRequestPage = obj({ items: arr(pullRequestSummary), page: int, hasMore: bool });

export const prThreadAnchor = obj({ path: str, line: int, side: oneOf('old', 'new') });

export const prReviewThread = obj({
  id: str,
  status: oneOf('active', 'resolved', 'closed'),
  anchor: nullable(prThreadAnchor),
  outdated: bool,
  comments: arr(obj({ id: str, author: prUser, body: str, createdAt: nullable(str) })),
  canResolve: bool
});

export const prCapabilities = obj({
  provider,
  votes: arr(prVote),
  strategies: arr(prStrategy),
  autoComplete: bool,
  resolveThreads: bool,
  workItems: bool,
  draft: bool
});

export const prSupport = obj({
  supported: bool,
  remote: nullable(str),
  provider,
  providerLabel: str,
  host: str,
  repoLabel: str,
  capabilities: nullable(prCapabilities),
  reason: nullable(str)
});

export const prAction = obj({ done: bool, detail: nullable(pullRequestDetail) });

export const prDiff = obj({
  files: arr(prFileChange),
  diffs: arr(fileDiff),
  omitted: arr(obj({ path: str, reason: str }))
});
