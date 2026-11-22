//! Column view model (task T3.3, "Miller columns"): one column per folder
//! of the path, the next column showing what is selected in the previous.
//! Pure logic; the UI loads each column's entries and draws them.

use std::path::{Path, PathBuf};

/// One column: a folder and the entry chosen in it (by name).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Column {
    pub dir: PathBuf,
    pub chosen: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Miller {
    pub columns: Vec<Column>,
    /// The column with the keyboard focus.
    pub focus: usize,
}

impl Miller {
    /// Columns for `path`: its last `max` folders (at least the folder
    /// itself), each one choosing the next; the focus on the last.
    pub fn new(path: &Path, max: usize) -> Self {
        let mut dirs: Vec<PathBuf> = path.ancestors().map(Path::to_path_buf).collect();
        dirs.reverse();
        let skip = dirs.len().saturating_sub(max.max(1));
        let dirs = &dirs[skip..];
        let columns: Vec<Column> = dirs
            .iter()
            .enumerate()
            .map(|(k, d)| Column {
                dir: d.clone(),
                chosen: dirs
                    .get(k + 1)
                    .and_then(|next| next.file_name())
                    .map(|n| n.to_string_lossy().into_owned()),
            })
            .collect();
        let focus = columns.len() - 1;
        Self { columns, focus }
    }

    /// The folder of the focused column.
    pub fn current_dir(&self) -> &Path {
        &self.columns[self.focus].dir
    }

    /// Chooses `name` in column `k` (focusing it); a folder opens the next
    /// column, a file closes the columns after `k`.
    pub fn choose(&mut self, k: usize, name: &str, is_dir: bool) {
        if k >= self.columns.len() {
            return;
        }
        self.columns.truncate(k + 1);
        self.columns[k].chosen = Some(name.to_owned());
        self.focus = k;
        if is_dir {
            let dir = self.columns[k].dir.join(name);
            self.columns.push(Column { dir, chosen: None });
        }
    }

    /// → : into the column of the chosen folder, if there is one.
    pub fn right(&mut self) -> bool {
        if self.focus + 1 < self.columns.len() {
            self.focus += 1;
            true
        } else {
            false
        }
    }

    /// ← : back to the previous column; at the first one, adds the parent
    /// folder as a new first column (so ← keeps going up).
    pub fn left(&mut self) -> bool {
        if self.focus > 0 {
            self.focus -= 1;
            // The columns after the focused one's choice stay visible.
            return true;
        }
        let first = &self.columns[0];
        let Some(parent) = first.dir.parent() else {
            return false;
        };
        let chosen = first
            .dir
            .file_name()
            .map(|n| n.to_string_lossy().into_owned());
        self.columns.insert(
            0,
            Column {
                dir: parent.to_path_buf(),
                chosen,
            },
        );
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dirs(m: &Miller) -> Vec<String> {
        m.columns
            .iter()
            .map(|c| c.dir.display().to_string())
            .collect()
    }

    #[test]
    fn columns_from_a_path() {
        let m = Miller::new(Path::new(r"C:\Users\ana\Docs"), 3);
        assert_eq!(
            dirs(&m),
            [r"C:\Users", r"C:\Users\ana", r"C:\Users\ana\Docs"]
        );
        assert_eq!(m.columns[0].chosen.as_deref(), Some("ana"));
        assert_eq!(m.columns[1].chosen.as_deref(), Some("Docs"));
        assert_eq!(m.columns[2].chosen, None);
        assert_eq!(m.focus, 2);
        assert_eq!(Miller::new(Path::new(r"C:\"), 3).columns.len(), 1);
    }

    #[test]
    fn choosing_opens_and_closes_columns() {
        let mut m = Miller::new(Path::new(r"C:\a\b"), 4);
        // Columns: C:\, C:\a, C:\a\b.
        m.choose(2, "c", true);
        assert_eq!(dirs(&m).last().unwrap(), r"C:\a\b\c");
        assert_eq!(m.focus, 2);
        m.choose(0, "x", true);
        assert_eq!(
            dirs(&m),
            [r"C:\", r"C:\x"],
            "columns after the choice close"
        );
        m.choose(1, "file.txt", false);
        assert_eq!(m.columns.len(), 2);
        assert_eq!(m.columns[1].chosen.as_deref(), Some("file.txt"));
    }

    #[test]
    fn left_and_right() {
        let mut m = Miller::new(Path::new(r"C:\a\b"), 2);
        assert_eq!(dirs(&m), [r"C:\a", r"C:\a\b"]);
        assert!(!m.right());
        assert!(m.left());
        assert_eq!(m.current_dir(), Path::new(r"C:\a"));
        assert!(m.left(), "at the first column: the parent is added");
        assert_eq!(dirs(&m), [r"C:\", r"C:\a", r"C:\a\b"]);
        assert_eq!(m.columns[0].chosen.as_deref(), Some("a"));
        assert!(!m.left() || m.columns[0].dir.parent().is_none());
        assert!(m.right());
    }
}
