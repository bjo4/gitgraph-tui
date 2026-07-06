//! Plain data types shared across layers. git2 types never leak past src/git/.

pub type CommitId = String; // 40-char hex

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitInfo {
    pub id: CommitId,
    pub short_id: String, // first 7 chars
    pub parents: Vec<CommitId>,
    pub summary: String,
    pub message: String,
    pub author_name: String,
    pub author_email: String,
    pub timestamp: i64, // unix seconds (author time)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefKind {
    Head,
    LocalBranch,
    RemoteBranch,
    Tag,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RefInfo {
    /// Short display name, e.g. "main", "origin/main", "v1.0", "HEAD".
    pub name: String,
    /// Full reference name, e.g. "refs/heads/main"; "HEAD" for the Head entry.
    pub refname: String,
    pub kind: RefKind,
    pub target: CommitId,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChangeKind {
    Added,
    Modified,
    Deleted,
    Renamed { from: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileChange {
    pub path: String,
    pub kind: ChangeKind,
    pub additions: usize,
    pub deletions: usize,
    pub is_binary: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffLine {
    /// git2 line origin: '+', '-', ' ' (context), '@' (hunk header), 'B' (binary marker).
    pub origin: char,
    pub content: String,
}
