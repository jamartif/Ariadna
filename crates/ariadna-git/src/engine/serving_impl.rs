//! `impl GitServing for GitCliEngine`: solo reenvía a los métodos inherentes de `core_ops` y
//! `branch_ops` (que tienen exactamente la misma firma). Un método inherente tiene prioridad
//! sobre el del trait, así que `self.nombre(..)` no recurre a sí mismo.

use std::path::Path;

use async_trait::async_trait;

use super::GitCliEngine;
use crate::error::GitResult;
use crate::models::{
    Branch, Commit, ConflictFile, DiffHunk, DiffScope, FileDiff, GitPath, GitStatus,
    IntegrationState, MergeFastForward, MergeResult, Remote, RemoteCheckout, Repository,
};
use crate::serving::{Confirmation, GitServing};
use crate::stash::{StashApplyResult, StashEntry, StashPushOptions};
use crate::tag::Tag;

#[async_trait]
impl GitServing for GitCliEngine {
    async fn open_repository(&self, path: &Path) -> GitResult<Repository> {
        self.open_repository(path).await
    }
    async fn status(&self, repo: &Repository, include_ignored: bool) -> GitResult<GitStatus> {
        self.status(repo, include_ignored).await
    }
    async fn stage(&self, repo: &Repository, paths: &[GitPath]) -> GitResult<()> {
        self.stage(repo, paths).await
    }
    async fn unstage(&self, repo: &Repository, paths: &[GitPath]) -> GitResult<()> {
        self.unstage(repo, paths).await
    }
    async fn stage_hunk(
        &self,
        repo: &Repository,
        hunk: &DiffHunk,
        path: &GitPath,
    ) -> GitResult<()> {
        self.stage_hunk(repo, hunk, path).await
    }
    async fn unstage_hunk(
        &self,
        repo: &Repository,
        hunk: &DiffHunk,
        path: &GitPath,
    ) -> GitResult<()> {
        self.unstage_hunk(repo, hunk, path).await
    }
    async fn stage_lines(
        &self,
        repo: &Repository,
        hunk: &DiffHunk,
        lines: &[u32],
        path: &GitPath,
    ) -> GitResult<()> {
        self.stage_lines(repo, hunk, lines, path).await
    }
    async fn unstage_lines(
        &self,
        repo: &Repository,
        hunk: &DiffHunk,
        lines: &[u32],
        path: &GitPath,
    ) -> GitResult<()> {
        self.unstage_lines(repo, hunk, lines, path).await
    }
    async fn stash_list(&self, repo: &Repository) -> GitResult<Vec<StashEntry>> {
        self.stash_list(repo).await
    }
    async fn stash_push(
        &self,
        repo: &Repository,
        options: &StashPushOptions,
    ) -> GitResult<StashEntry> {
        self.stash_push(repo, options).await
    }
    async fn stash_apply(&self, repo: &Repository, oid: &str) -> GitResult<StashApplyResult> {
        self.stash_apply(repo, oid).await
    }
    async fn stash_pop(&self, repo: &Repository, oid: &str) -> GitResult<StashApplyResult> {
        self.stash_pop(repo, oid).await
    }
    async fn stash_drop(
        &self,
        repo: &Repository,
        oid: &str,
        confirmation: &Confirmation,
    ) -> GitResult<()> {
        self.stash_drop(repo, oid, confirmation).await
    }
    async fn stash_diff(&self, repo: &Repository, oid: &str) -> GitResult<Vec<FileDiff>> {
        self.stash_diff(repo, oid).await
    }
    async fn commit(
        &self,
        repo: &Repository,
        message: &str,
        allow_detached: bool,
    ) -> GitResult<String> {
        self.commit(repo, message, allow_detached).await
    }
    async fn amend_last_commit(&self, repo: &Repository, message: &str) -> GitResult<String> {
        self.amend_last_commit(repo, message).await
    }
    async fn log(&self, repo: &Repository, limit: usize) -> GitResult<Vec<Commit>> {
        self.log(repo, limit).await
    }
    async fn log_page(
        &self,
        repo: &Repository,
        skip: usize,
        limit: usize,
    ) -> GitResult<Vec<Commit>> {
        self.log_page(repo, skip, limit).await
    }
    async fn log_after(
        &self,
        repo: &Repository,
        cursor: &[String],
        limit: usize,
    ) -> GitResult<Vec<Commit>> {
        self.log_after(repo, cursor, limit).await
    }
    async fn tags(&self, repo: &Repository) -> GitResult<Vec<Tag>> {
        self.tags(repo).await
    }
    async fn create_tag(
        &self,
        repo: &Repository,
        name: &str,
        target: Option<&str>,
        message: Option<&str>,
    ) -> GitResult<Tag> {
        self.create_tag(repo, name, target, message).await
    }
    async fn delete_tag(
        &self,
        repo: &Repository,
        name: &str,
        confirmation: &Confirmation,
    ) -> GitResult<()> {
        self.delete_tag(repo, name, confirmation).await
    }
    async fn push_tag(&self, repo: &Repository, remote: &str, name: &str) -> GitResult<()> {
        self.push_tag(repo, remote, name).await
    }
    async fn delete_remote_tag(
        &self,
        repo: &Repository,
        remote: &str,
        name: &str,
        confirmation: &Confirmation,
    ) -> GitResult<()> {
        self.delete_remote_tag(repo, remote, name, confirmation)
            .await
    }
    async fn search_log(
        &self,
        repo: &Repository,
        query: &str,
        limit: usize,
    ) -> GitResult<Vec<Commit>> {
        self.search_log(repo, query, limit).await
    }
    async fn diff(
        &self,
        repo: &Repository,
        scope: &DiffScope,
        paths: &[GitPath],
    ) -> GitResult<Vec<FileDiff>> {
        self.diff(repo, scope, paths).await
    }
    async fn branches(&self, repo: &Repository) -> GitResult<Vec<Branch>> {
        self.branches(repo).await
    }
    async fn remote_tracking_branches(&self, repo: &Repository) -> GitResult<Vec<Branch>> {
        self.remote_tracking_branches(repo).await
    }
    async fn create_branch(
        &self,
        repo: &Repository,
        name: &str,
        start_point: Option<&str>,
    ) -> GitResult<()> {
        self.create_branch(repo, name, start_point).await
    }
    async fn rename_branch(&self, repo: &Repository, old: &str, new: &str) -> GitResult<()> {
        self.rename_branch(repo, old, new).await
    }
    async fn checkout(
        &self,
        repo: &Repository,
        reference: &str,
        allow_diverged_local: bool,
    ) -> GitResult<()> {
        self.checkout(repo, reference, allow_diverged_local).await
    }
    async fn checkout_remote_ref(
        &self,
        repo: &Repository,
        remote: &str,
        reference: &str,
        local_branch: &str,
        allow_moved: bool,
    ) -> GitResult<RemoteCheckout> {
        self.checkout_remote_ref(repo, remote, reference, local_branch, allow_moved)
            .await
    }
    async fn delete_branch(&self, repo: &Repository, name: &str, force: bool) -> GitResult<()> {
        self.delete_branch(repo, name, force).await
    }
    async fn merge(
        &self,
        repo: &Repository,
        branch: &str,
        fast_forward: MergeFastForward,
    ) -> GitResult<MergeResult> {
        self.merge(repo, branch, fast_forward).await
    }
    async fn integration_state(&self, repo: &Repository) -> GitResult<IntegrationState> {
        self.integration_state(repo).await
    }
    async fn abort_integration(&self, repo: &Repository) -> GitResult<()> {
        self.abort_integration(repo).await
    }
    async fn continue_integration(&self, repo: &Repository) -> GitResult<()> {
        self.continue_integration(repo).await
    }
    async fn conflicts(&self, repo: &Repository) -> GitResult<Vec<ConflictFile>> {
        self.conflicts(repo).await
    }
    async fn write_working_tree_file(
        &self,
        repo: &Repository,
        relative_path: &GitPath,
        contents: &[u8],
        confirmation: &Confirmation,
    ) -> GitResult<()> {
        self.write_working_tree_file(repo, relative_path, contents, confirmation)
            .await
    }
    async fn remove_working_tree_file(
        &self,
        repo: &Repository,
        relative_path: &GitPath,
        confirmation: &Confirmation,
    ) -> GitResult<()> {
        self.remove_working_tree_file(repo, relative_path, confirmation)
            .await
    }
    async fn mark_resolved(&self, repo: &Repository, path: &GitPath) -> GitResult<()> {
        self.mark_resolved(repo, path).await
    }
    async fn restore_worktree(
        &self,
        repo: &Repository,
        paths: &[GitPath],
        confirmation: &Confirmation,
    ) -> GitResult<()> {
        self.restore_worktree(repo, paths, confirmation).await
    }
    async fn list_remotes(&self, repo: &Repository) -> GitResult<Vec<Remote>> {
        self.list_remotes(repo).await
    }
    async fn add_remote(&self, repo: &Repository, name: &str, url: &str) -> GitResult<()> {
        self.add_remote(repo, name, url).await
    }
    async fn rename_remote(&self, repo: &Repository, old: &str, new: &str) -> GitResult<()> {
        self.rename_remote(repo, old, new).await
    }
    async fn remove_remote(&self, repo: &Repository, name: &str) -> GitResult<()> {
        self.remove_remote(repo, name).await
    }
    async fn fetch(&self, repo: &Repository, remote: &str) -> GitResult<()> {
        self.fetch(repo, remote).await
    }
    async fn pull(
        &self,
        repo: &Repository,
        remote: &str,
        refspec: Option<&str>,
        rebase: bool,
    ) -> GitResult<MergeResult> {
        self.pull(repo, remote, refspec, rebase).await
    }
    async fn push(
        &self,
        repo: &Repository,
        remote: &str,
        refspec: Option<&str>,
        set_upstream: bool,
    ) -> GitResult<()> {
        self.push(repo, remote, refspec, set_upstream).await
    }
}
