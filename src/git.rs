//! Git operations for syncing document references.
//!
//! Provides utilities for working with git repositories:
//! - Get current HEAD SHA
//! - Get file content at specific commits
//! - Get diff between commits
//! - List changed files

use std::path::Path;

use git2::{DiffOptions, Repository};

use crate::error::AppError;

/// A changed file in a diff.
#[derive(Debug, Clone)]
pub struct ChangedFile {
    /// Path to the file.
    pub path: String,
    /// Type of change.
    pub change_type: ChangeType,
}

/// Type of change to a file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangeType {
    Added,
    Modified,
    Deleted,
    Renamed,
}

/// A line-level diff hunk.
#[derive(Debug, Clone)]
pub struct DiffHunk {
    /// Starting line in old file (1-indexed).
    pub old_start: u32,
    /// Number of lines in old file.
    pub old_lines: u32,
    /// Starting line in new file (1-indexed).
    pub new_start: u32,
    /// Number of lines in new file.
    pub new_lines: u32,
}

/// Diff information for a file.
#[derive(Debug, Clone)]
pub struct FileDiff {
    /// Path to the file.
    pub path: String,
    /// List of changed hunks.
    pub hunks: Vec<DiffHunk>,
}

/// Git operations helper.
pub struct GitOps {
    repo: Repository,
}

impl GitOps {
    /// Open a git repository at the given path.
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self, AppError> {
        let repo = Repository::discover(path).map_err(|e| AppError::GitMessage {
            message: format!("Failed to open repository: {}", e),
        })?;
        Ok(Self { repo })
    }

    /// Open a git repository in the current directory.
    pub fn open_current() -> Result<Self, AppError> {
        Self::open(".")
    }

    /// Get the current HEAD commit SHA.
    pub fn get_head_sha(&self) -> Result<String, AppError> {
        let head = self.repo.head().map_err(|e| AppError::GitMessage {
            message: format!("Failed to get HEAD: {}", e),
        })?;

        let commit = head.peel_to_commit().map_err(|e| AppError::GitMessage {
            message: format!("Failed to get HEAD commit: {}", e),
        })?;

        Ok(commit.id().to_string())
    }

    /// Get file content at a specific commit.
    ///
    /// Returns `None` if the file doesn't exist at that commit.
    pub fn get_content_at_commit(
        &self,
        path: &str,
        commit_sha: &str,
    ) -> Result<Option<String>, AppError> {
        let oid = git2::Oid::from_str(commit_sha).map_err(|e| AppError::GitMessage {
            message: format!("Invalid commit SHA '{}': {}", commit_sha, e),
        })?;

        let commit = self
            .repo
            .find_commit(oid)
            .map_err(|e| AppError::GitMessage {
                message: format!("Commit not found '{}': {}", commit_sha, e),
            })?;

        let tree = commit.tree().map_err(|e| AppError::GitMessage {
            message: format!("Failed to get tree: {}", e),
        })?;

        match tree.get_path(Path::new(path)) {
            Ok(entry) => {
                let blob = self
                    .repo
                    .find_blob(entry.id())
                    .map_err(|e| AppError::GitMessage {
                        message: format!("Failed to get blob: {}", e),
                    })?;

                if blob.is_binary() {
                    return Ok(None);
                }

                let content =
                    std::str::from_utf8(blob.content()).map_err(|e| AppError::GitMessage {
                        message: format!("File is not valid UTF-8: {}", e),
                    })?;

                Ok(Some(content.to_string()))
            }
            Err(_) => Ok(None), // File doesn't exist at this commit
        }
    }

    /// Get list of files changed between two commits.
    ///
    /// If `from_sha` is None, returns all files in `to_sha`.
    /// If `to_sha` is None, uses HEAD.
    pub fn get_changed_files(
        &self,
        from_sha: Option<&str>,
        to_sha: Option<&str>,
    ) -> Result<Vec<ChangedFile>, AppError> {
        let to_commit = match to_sha {
            Some(sha) => {
                let oid = git2::Oid::from_str(sha).map_err(|e| AppError::GitMessage {
                    message: format!("Invalid to_sha '{}': {}", sha, e),
                })?;
                self.repo
                    .find_commit(oid)
                    .map_err(|e| AppError::GitMessage {
                        message: format!("Commit not found '{}': {}", sha, e),
                    })?
            }
            None => {
                let head = self.repo.head().map_err(|e| AppError::GitMessage {
                    message: format!("Failed to get HEAD: {}", e),
                })?;
                head.peel_to_commit().map_err(|e| AppError::GitMessage {
                    message: format!("Failed to get HEAD commit: {}", e),
                })?
            }
        };

        let to_tree = to_commit.tree().map_err(|e| AppError::GitMessage {
            message: format!("Failed to get tree: {}", e),
        })?;

        let from_tree = match from_sha {
            Some(sha) => {
                let oid = git2::Oid::from_str(sha).map_err(|e| AppError::GitMessage {
                    message: format!("Invalid from_sha '{}': {}", sha, e),
                })?;
                let commit = self
                    .repo
                    .find_commit(oid)
                    .map_err(|e| AppError::GitMessage {
                        message: format!("Commit not found '{}': {}", sha, e),
                    })?;
                Some(commit.tree().map_err(|e| AppError::GitMessage {
                    message: format!("Failed to get tree: {}", e),
                })?)
            }
            None => None,
        };

        let diff = self
            .repo
            .diff_tree_to_tree(from_tree.as_ref(), Some(&to_tree), None)
            .map_err(|e| AppError::GitMessage {
                message: format!("Failed to compute diff: {}", e),
            })?;

        let mut files = Vec::new();
        diff.foreach(
            &mut |delta, _| {
                let path = delta
                    .new_file()
                    .path()
                    .or_else(|| delta.old_file().path())
                    .map(|p| p.to_string_lossy().to_string());

                if let Some(path) = path {
                    let change_type = match delta.status() {
                        git2::Delta::Added => ChangeType::Added,
                        git2::Delta::Deleted => ChangeType::Deleted,
                        git2::Delta::Modified => ChangeType::Modified,
                        git2::Delta::Renamed => ChangeType::Renamed,
                        _ => ChangeType::Modified,
                    };
                    files.push(ChangedFile { path, change_type });
                }
                true
            },
            None,
            None,
            None,
        )
        .map_err(|e| AppError::GitMessage {
            message: format!("Failed to iterate diff: {}", e),
        })?;

        Ok(files)
    }

    /// Get detailed diff for a specific file between two commits.
    ///
    /// Returns the hunks (changed regions) in the file.
    pub fn get_file_diff(
        &self,
        path: &str,
        from_sha: &str,
        to_sha: Option<&str>,
    ) -> Result<Option<FileDiff>, AppError> {
        let from_oid = git2::Oid::from_str(from_sha).map_err(|e| AppError::GitMessage {
            message: format!("Invalid from_sha '{}': {}", from_sha, e),
        })?;
        let from_commit = self
            .repo
            .find_commit(from_oid)
            .map_err(|e| AppError::GitMessage {
                message: format!("Commit not found '{}': {}", from_sha, e),
            })?;
        let from_tree = from_commit.tree().map_err(|e| AppError::GitMessage {
            message: format!("Failed to get tree: {}", e),
        })?;

        let to_commit = match to_sha {
            Some(sha) => {
                let oid = git2::Oid::from_str(sha).map_err(|e| AppError::GitMessage {
                    message: format!("Invalid to_sha '{}': {}", sha, e),
                })?;
                self.repo
                    .find_commit(oid)
                    .map_err(|e| AppError::GitMessage {
                        message: format!("Commit not found '{}': {}", sha, e),
                    })?
            }
            None => {
                let head = self.repo.head().map_err(|e| AppError::GitMessage {
                    message: format!("Failed to get HEAD: {}", e),
                })?;
                head.peel_to_commit().map_err(|e| AppError::GitMessage {
                    message: format!("Failed to get HEAD commit: {}", e),
                })?
            }
        };
        let to_tree = to_commit.tree().map_err(|e| AppError::GitMessage {
            message: format!("Failed to get tree: {}", e),
        })?;

        let mut opts = DiffOptions::new();
        opts.pathspec(path);

        let diff = self
            .repo
            .diff_tree_to_tree(Some(&from_tree), Some(&to_tree), Some(&mut opts))
            .map_err(|e| AppError::GitMessage {
                message: format!("Failed to compute diff: {}", e),
            })?;

        let mut hunks = Vec::new();
        let mut found_file = false;

        diff.foreach(
            &mut |_, _| {
                found_file = true;
                true
            },
            None,
            Some(&mut |_, hunk| {
                hunks.push(DiffHunk {
                    old_start: hunk.old_start(),
                    old_lines: hunk.old_lines(),
                    new_start: hunk.new_start(),
                    new_lines: hunk.new_lines(),
                });
                true
            }),
            None,
        )
        .map_err(|e| AppError::GitMessage {
            message: format!("Failed to iterate diff: {}", e),
        })?;

        if found_file {
            Ok(Some(FileDiff {
                path: path.to_string(),
                hunks,
            }))
        } else {
            Ok(None)
        }
    }

    /// Check if a line range overlaps with any diff hunks.
    ///
    /// Used to detect if a document reference is in a changed region.
    pub fn is_in_changed_region(hunks: &[DiffHunk], start_line: u32, end_line: u32) -> bool {
        for hunk in hunks {
            // Check if the reference's line range overlaps with the hunk's old range
            let hunk_end = hunk.old_start + hunk.old_lines.saturating_sub(1);
            if start_line <= hunk_end && end_line >= hunk.old_start {
                return true;
            }
        }
        false
    }
}
