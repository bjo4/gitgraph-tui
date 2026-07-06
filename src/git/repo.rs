use std::collections::HashMap;
use std::path::Path;

use anyhow::{Context, Result};
use git2::{Oid, Repository, Sort};

use super::types::{CommitId, CommitInfo, RefInfo, RefKind};

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

    /// Walk the whole commit DAG (oids only — cheap even for huge repos).
    /// `filter`: a full refname such as "refs/heads/main" limits the walk
    /// to commits reachable from that ref; None walks every ref plus HEAD.
    pub fn commit_ids(&self, filter: Option<&str>) -> Result<Vec<CommitId>> {
        let mut walk = self.inner.revwalk()?;
        walk.set_sorting(Sort::TOPOLOGICAL | Sort::TIME)?;
        match filter {
            Some(refname) => {
                walk.push_ref(refname)
                    .with_context(|| format!("unknown ref: {refname}"))?;
            }
            None => {
                walk.push_glob("refs/heads/*")?;
                walk.push_glob("refs/tags/*")?;
                walk.push_glob("refs/remotes/*")?;
                if self.inner.head().is_ok() {
                    walk.push_head()?;
                }
            }
        }
        Ok(walk
            .filter_map(|o| o.ok())
            .map(|oid| oid.to_string())
            .collect())
    }

    /// Convert one chunk of commit ids into full CommitInfo values.
    pub fn load_commits(&self, ids: &[CommitId]) -> Result<Vec<CommitInfo>> {
        ids.iter()
            .map(|id| {
                let oid = Oid::from_str(id).context("invalid commit id")?;
                let c = self
                    .inner
                    .find_commit(oid)
                    .with_context(|| format!("commit {id} not found"))?;
                let author = c.author();
                Ok(CommitInfo {
                    short_id: id[..7].to_string(),
                    id: id.clone(),
                    parents: c.parent_ids().map(|p| p.to_string()).collect(),
                    summary: c.summary().ok().flatten().unwrap_or("").to_string(),
                    message: c.message().unwrap_or("").to_string(),
                    author_name: author.name().unwrap_or("").to_string(),
                    author_email: author.email().unwrap_or("").to_string(),
                    timestamp: author.when().seconds(),
                })
            })
            .collect()
    }
}
