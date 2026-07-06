//! Pure lane-assignment layout: commit DAG (topological order) → drawable
//! rows of colored glyphs. No IO, no git2, no ratatui.
use crate::git::types::{CommitId, CommitInfo};

pub const PALETTE_SIZE: usize = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cell {
    pub glyph: char,
    /// Palette index 0..PALETTE_SIZE. The UI maps it to a real color.
    pub color: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphRow {
    /// Lane of the commit dot. Even cell index = 2 * lane.
    pub lane: usize,
    pub color: usize,
    pub cells: Vec<Cell>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Lane {
    waiting_for: CommitId,
    color: usize,
}

/// Keeps open-lane state between chunks so loading is incremental.
#[derive(Debug, Default)]
pub struct LayoutEngine {
    slots: Vec<Option<Lane>>,
    next_color: usize,
}

impl LayoutEngine {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn reset(&mut self) {
        self.slots.clear();
        self.next_color = 0;
    }

    /// Process the next chunk of commits (topological order), continuing
    /// from the lane state left by previous calls.
    pub fn process(&mut self, commits: &[CommitInfo]) -> Vec<GraphRow> {
        commits.iter().map(|c| self.process_one(c)).collect()
    }

    fn process_one(&mut self, commit: &CommitInfo) -> GraphRow {
        let waiting: Vec<usize> = (0..self.slots.len())
            .filter(|&i| matches!(&self.slots[i], Some(l) if l.waiting_for == commit.id))
            .collect();
        // The commit sits on the leftmost lane that waits for it, or a new one.
        let (lane, color) = match waiting.first() {
            Some(&i) => (i, self.slots[i].as_ref().unwrap().color),
            None => self.alloc(commit.id.clone(), &[]),
        };
        // Other lanes waiting for this commit join it here and close.
        let closes: Vec<(usize, usize)> = waiting
            .iter()
            .skip(1) // waiting[0] is the commit's own lane; slicing [1..] would panic when empty
            .map(|&i| (i, self.slots[i].as_ref().unwrap().color))
            .collect();
        for &(i, _) in &closes {
            self.slots[i] = None;
        }
        // Lanes that continue straight through this row.
        let pass: Vec<(usize, usize)> = (0..self.slots.len())
            .filter(|&i| i != lane)
            .filter_map(|i| self.slots[i].as_ref().map(|l| (i, l.color)))
            .collect();
        // First parent inherits the commit's lane; a root closes it.
        match commit.parents.first() {
            Some(p) => {
                self.slots[lane] = Some(Lane {
                    waiting_for: p.clone(),
                    color,
                });
            }
            None => self.slots[lane] = None,
        }
        // Remaining parents (merge sources) each open a new lane. Avoid slots
        // closed on this very row so the closing glyph stays visible.
        let closed_now: Vec<usize> = closes.iter().map(|&(i, _)| i).collect();
        let opens: Vec<(usize, usize)> = commit
            .parents
            .iter()
            .skip(1)
            .map(|p| self.alloc(p.clone(), &closed_now))
            .collect();
        self.trim();
        let cells = render_cells(lane, color, &closes, &opens, &pass);
        GraphRow { lane, color, cells }
    }

    /// Allocate the leftmost free slot not in `avoid`; assign the next
    /// palette color. Returns (lane index, color).
    fn alloc(&mut self, waiting_for: CommitId, avoid: &[usize]) -> (usize, usize) {
        let color = self.next_color % PALETTE_SIZE;
        self.next_color += 1;
        let free = (0..self.slots.len()).find(|i| self.slots[*i].is_none() && !avoid.contains(i));
        let lane = match free {
            Some(i) => i,
            None => {
                self.slots.push(None);
                self.slots.len() - 1
            }
        };
        self.slots[lane] = Some(Lane { waiting_for, color });
        (lane, color)
    }

    fn trim(&mut self) {
        while matches!(self.slots.last(), Some(None)) {
            self.slots.pop();
        }
    }
}

/// Paint one row: pass-through verticals first, then connector curves,
/// finally the commit dot (dots always win).
fn render_cells(
    lane: usize,
    color: usize,
    closes: &[(usize, usize)],
    opens: &[(usize, usize)],
    pass: &[(usize, usize)],
) -> Vec<Cell> {
    let max_lane = std::iter::once(lane)
        .chain(closes.iter().map(|&(i, _)| i))
        .chain(opens.iter().map(|&(i, _)| i))
        .chain(pass.iter().map(|&(i, _)| i))
        .max()
        .unwrap_or(0);
    let mut cells = vec![
        Cell {
            glyph: ' ',
            color: 0
        };
        2 * (max_lane + 1)
    ];
    for &(i, c) in pass {
        cells[2 * i] = Cell {
            glyph: '│',
            color: c,
        };
    }
    for &(i, c) in closes {
        let end = if i > lane { '╯' } else { '╰' };
        connector(&mut cells, lane, i, c, end);
    }
    for &(i, c) in opens {
        let end = if i > lane { '╮' } else { '╭' };
        connector(&mut cells, lane, i, c, end);
    }
    cells[2 * lane] = Cell {
        glyph: '●', color
    };
    cells
}

/// Horizontal run from the commit's lane to `to`, ending in a curve glyph.
/// Crossing a vertical becomes '┼' (keeping the vertical's color); existing
/// curves from earlier connectors are left intact.
fn connector(cells: &mut [Cell], from: usize, to: usize, color: usize, end: char) {
    let (lo, hi) = (from.min(to), from.max(to));
    for cell in &mut cells[(2 * lo + 1)..(2 * hi)] {
        *cell = match cell.glyph {
            '│' | '┼' => Cell {
                glyph: '┼',
                color: cell.color,
            },
            ' ' | '─' => Cell {
                glyph: '─', color
            },
            other => Cell {
                glyph: other,
                color: cell.color,
            },
        };
    }
    cells[2 * to] = Cell { glyph: end, color };
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::types::CommitInfo;

    /// Minimal commit for layout purposes.
    fn c(id: &str, parents: &[&str]) -> CommitInfo {
        CommitInfo {
            id: id.to_string(),
            short_id: id.chars().take(7).collect(),
            parents: parents.iter().map(|s| s.to_string()).collect(),
            summary: format!("commit {id}"),
            message: String::new(),
            author_name: "t".to_string(),
            author_email: "t@t".to_string(),
            timestamp: 0,
        }
    }

    /// Rows as glyph strings (trailing spaces trimmed) — readable assertions.
    fn glyphs(rows: &[GraphRow]) -> Vec<String> {
        rows.iter()
            .map(|r| {
                r.cells
                    .iter()
                    .map(|c| c.glyph)
                    .collect::<String>()
                    .trim_end()
                    .to_string()
            })
            .collect()
    }

    #[test]
    fn linear_history_stays_in_lane_zero() {
        let mut e = LayoutEngine::new();
        let rows = e.process(&[c("c3", &["c2"]), c("c2", &["c1"]), c("c1", &[])]);
        assert_eq!(glyphs(&rows), vec!["●", "●", "●"]);
        assert!(rows.iter().all(|r| r.lane == 0 && r.color == 0));
    }

    #[test]
    fn branch_forks_then_joins() {
        // main: m2→m1 · feature: f1→m1 · order: m2, f1, m1
        let mut e = LayoutEngine::new();
        let rows = e.process(&[c("m2", &["m1"]), c("f1", &["m1"]), c("m1", &[])]);
        assert_eq!(glyphs(&rows), vec!["●", "│ ●", "●─╯"]);
        assert_eq!(rows[1].lane, 1);
        assert_eq!(rows[1].color, 1); // second lane gets the next palette color
        assert_eq!(rows[2].lane, 0);
    }

    #[test]
    fn merge_commit_opens_a_lane_for_its_second_parent() {
        // m3 = merge(m2, f1); m2→m1; f1→m1; m1 root
        let mut e = LayoutEngine::new();
        let rows = e.process(&[
            c("m3", &["m2", "f1"]),
            c("m2", &["m1"]),
            c("f1", &["m1"]),
            c("m1", &[]),
        ]);
        assert_eq!(glyphs(&rows), vec!["●─╮", "● │", "│ ●", "●─╯"]);
    }

    #[test]
    fn octopus_merge_opens_multiple_lanes() {
        let mut e = LayoutEngine::new();
        let rows = e.process(&[
            c("o", &["a", "b", "x"]),
            c("a", &[]),
            c("b", &[]),
            c("x", &[]),
        ]);
        assert_eq!(glyphs(&rows), vec!["●─╮─╮", "● │ │", "  ● │", "    ●"]);
    }

    #[test]
    fn criss_cross_crossing_uses_the_cross_glyph() {
        // a=merge(c,d), b=merge(c,d) — the close of b's lane at c crosses
        // a's second-parent lane.
        let mut e = LayoutEngine::new();
        let rows = e.process(&[
            c("a", &["c", "d"]),
            c("b", &["c", "d"]),
            c("c", &[]),
            c("d", &[]),
        ]);
        assert_eq!(glyphs(&rows), vec!["●─╮", "│ │ ●─╮", "●─┼─╯ │", "  ●───╯"]);
    }

    #[test]
    fn disconnected_histories_reuse_freed_lanes() {
        let mut e = LayoutEngine::new();
        let rows = e.process(&[
            c("a2", &["a1"]),
            c("b2", &["b1"]),
            c("a1", &[]), // frees lane 0
            c("b1", &[]),
        ]);
        assert_eq!(glyphs(&rows), vec!["●", "│ ●", "● │", "  ●"]);
    }

    #[test]
    fn chunked_processing_matches_single_pass() {
        let commits = [
            c("m3", &["m2", "f1"]),
            c("m2", &["m1"]),
            c("f1", &["m1"]),
            c("m1", &[]),
        ];
        let mut whole = LayoutEngine::new();
        let all = whole.process(&commits);
        let mut chunked = LayoutEngine::new();
        let mut rows = chunked.process(&commits[..2]);
        rows.extend(chunked.process(&commits[2..]));
        assert_eq!(all, rows);
    }

    #[test]
    fn colors_cycle_through_the_palette() {
        // 9 parallel tips: the 9th lane wraps to color 0
        let mut e = LayoutEngine::new();
        let tips: Vec<CommitInfo> = (0..9).map(|i| c(&format!("t{i}"), &["root"])).collect();
        let rows = e.process(&tips);
        assert_eq!(rows[8].color, 8 % PALETTE_SIZE);
        assert_eq!(rows[8].lane, 8);
    }

    #[test]
    fn reset_clears_lane_state() {
        let mut e = LayoutEngine::new();
        e.process(&[c("a", &["p"])]);
        e.reset();
        let rows = e.process(&[c("b", &[])]);
        assert_eq!(rows[0].lane, 0);
        assert_eq!(rows[0].color, 0);
    }
}
