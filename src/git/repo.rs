use std::collections::HashMap;
use std::path::Path;

use anyhow::{Context, Result};
use git2::Repository;

use super::types::{CommitId, RefInfo, RefKind};

pub struct GitRepo {
    pub(crate) inner: Repository,
}

// git2::Repository has no Debug impl, so it can't be derived; a minimal
// manual impl is enough to satisfy `Result<GitRepo, _>::unwrap_err()` in tests.
impl std::fmt::Debug for GitRepo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GitRepo")
            .field("name", &self.name())
            .finish()
    }
}

impl GitRepo {
    /// Open the repository containing `path` (walks up parent directories,
    /// like the git CLI).
    pub fn discover(path: &Path) -> Result<Self> {
        let inner = Repository::discover(path)
            .with_context(|| format!("not a git repository (or any parent): {}", path.display()))?;
        Ok(Self { inner })
    }

    /// Repository directory name, for the title bar.
    pub fn name(&self) -> String {
        let p = self.inner.workdir().unwrap_or_else(|| self.inner.path());
        p.file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "repo".to_string())
    }

    /// HEAD plus every local branch, remote branch, and tag, resolved to
    /// the commit each points at (annotated tags are peeled).
    pub fn refs(&self) -> Result<Vec<RefInfo>> {
        let mut out = Vec::new();
        if let Ok(head) = self.inner.head()
            && let Ok(commit) = head.peel_to_commit()
        {
            let name = if self.inner.head_detached().unwrap_or(false) {
                "HEAD (detached)".to_string()
            } else {
                "HEAD".to_string()
            };
            out.push(RefInfo {
                name,
                refname: "HEAD".to_string(),
                kind: RefKind::Head,
                target: commit.id().to_string(),
            });
        }
        for r in self.inner.references()? {
            let Ok(r) = r else { continue };
            let kind = if r.is_branch() {
                RefKind::LocalBranch
            } else if r.is_remote() {
                RefKind::RemoteBranch
            } else if r.is_tag() {
                RefKind::Tag
            } else {
                continue;
            };
            let (Ok(refname), Ok(short)) = (r.name(), r.shorthand()) else {
                continue;
            };
            let (refname, name) = (refname.to_string(), short.to_string());
            let Ok(commit) = r.peel_to_commit() else {
                continue;
            };
            out.push(RefInfo {
                name,
                refname,
                kind,
                target: commit.id().to_string(),
            });
        }
        Ok(out)
    }

    /// Group refs by the commit they point at, for per-row label lookup.
    pub fn ref_map(refs: &[RefInfo]) -> HashMap<CommitId, Vec<RefInfo>> {
        let mut map: HashMap<CommitId, Vec<RefInfo>> = HashMap::new();
        for r in refs {
            map.entry(r.target.clone()).or_default().push(r.clone());
        }
        map
    }
}
