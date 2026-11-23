//! Metadata columns and "Agrupar por" (T6.10, D-068).
//!
//! Groups: the grouped order is computed with the sort, on a worker; the
//! headers (where each group starts in the shown rows) are found on a worker
//! too, each time the rows change, and drawn as rows of their own in the
//! details view (the grid keeps the grouped order without headers).
//!
//! Columns: dimensions, duration and date taken, from the Windows Property
//! System, read on a thread for the rows drawn only, cached by name; a cloud
//! file is never downloaded for them.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use futures::StreamExt;
use fx_core::FileList;
use fx_core::grouping::{self, Facts, Group, GroupBy, Now};
use fx_core::{Direction, SortKey, tr};
use gpui::{AppContext, Context, SharedString};

use super::FolderView;

/// The grouping of a tab and its headers.
#[derive(Debug, Default)]
pub(super) struct Groups {
    pub(super) by: GroupBy,
    /// `(position in the rows, group)`, by position.
    headers: Vec<(usize, Group)>,
    /// Display index of each header (its position plus the headers above).
    at: Vec<usize>,
    /// Bumped each time the shown rows change.
    pub(super) rows_version: u64,
    /// The `rows_version` the headers were found for.
    version: u64,
    running: bool,
}

/// What a line of the details view shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Slot {
    /// The header of group `k`.
    Header(usize),
    /// The row at this position.
    Row(usize),
}

pub(super) fn now() -> Now {
    let t = SystemTime::now();
    Now {
        secs: t
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_secs() as i64),
        offset: fx_shell::utc_offset_at(t),
    }
}

fn facts(list: &FileList, i: u32) -> Facts<'_> {
    Facts {
        name: list.name(i),
        is_dir: list.attributes(i).is_dir(),
        size: list.size(i),
        modified: list
            .modified(i)
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_secs() as i64),
    }
}

/// The display order: sorted, then grouped (on a worker).
pub(super) fn order_for(
    list: &FileList,
    key: SortKey,
    direction: Direction,
    by: GroupBy,
    now: Now,
) -> Vec<u32> {
    let order = list.sorted_order(key, direction);
    grouping::grouped_order(order, by, now, |i| facts(list, i))
}

/// The optional media columns.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaCol {
    Dimensions,
    Duration,
    Taken,
}

impl MediaCol {
    pub const ALL: [MediaCol; 3] = [MediaCol::Dimensions, MediaCol::Duration, MediaCol::Taken];

    pub(super) fn index(self) -> usize {
        self as usize
    }

    fn property(self) -> &'static str {
        match self {
            MediaCol::Dimensions => "System.Image.Dimensions",
            MediaCol::Duration => "System.Media.Duration",
            MediaCol::Taken => "System.Photo.DateTaken",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            MediaCol::Dimensions => tr!("Dimensões", "Dimensions"),
            MediaCol::Duration => tr!("Duração", "Duration"),
            MediaCol::Taken => tr!("Data da foto", "Date taken"),
        }
    }

    pub(super) fn default_width(self) -> f32 {
        match self {
            MediaCol::Dimensions => 110.0,
            MediaCol::Duration => 80.0,
            MediaCol::Taken => 150.0,
        }
    }
}

type Values = [Option<SharedString>; 3];

/// The media columns of a tab and the values read so far.
#[derive(Debug, Default)]
pub(super) struct Media {
    pub(super) shown: [bool; 3],
    /// The folder the values belong to.
    dir: PathBuf,
    values: HashMap<String, Values>,
    /// Names waiting to be read, and the ones asked already.
    wanted: Vec<String>,
    asked: HashSet<String>,
    running: bool,
}

impl Media {
    pub(super) fn any(&self) -> bool {
        self.shown.iter().any(|s| *s)
    }
}

impl FolderView {
    /// Headers that fit the current rows (stale ones past the end are left
    /// out until the new ones arrive).
    fn live_headers(&self) -> usize {
        if self.groups.by == GroupBy::None || self.grid {
            return 0;
        }
        let n = self.rows().len();
        self.groups.headers.partition_point(|(p, _)| *p < n)
    }

    /// Lines of the list: rows of tiles in the grid, rows plus headers in
    /// the details view.
    pub(super) fn list_count(&self) -> usize {
        if self.grid {
            self.rows().len().div_ceil(self.grid_cols)
        } else {
            self.rows().len() + self.live_headers()
        }
    }

    /// The list line that shows the row at `pos`.
    pub(super) fn list_item(&self, pos: usize) -> usize {
        if self.grid {
            return pos / self.grid_cols;
        }
        let n = self.live_headers();
        pos + self.groups.headers[..n].partition_point(|(p, _)| *p <= pos)
    }

    /// What list line `line` shows (details view).
    pub(super) fn slot(&self, line: usize) -> Slot {
        let at = &self.groups.at[..self.live_headers()];
        let k = at.partition_point(|&x| x <= line);
        if k > 0 && at[k - 1] == line {
            Slot::Header(k - 1)
        } else {
            Slot::Row(line - k)
        }
    }

    pub(super) fn group_label(&self, k: usize) -> String {
        self.groups
            .headers
            .get(k)
            .map(|(_, g)| g.label())
            .unwrap_or_default()
    }

    /// The group a row is in, for screen readers.
    pub(super) fn group_of_pos(&self, pos: usize) -> Option<String> {
        let n = self.live_headers();
        let k = self.groups.headers[..n].partition_point(|(p, _)| *p <= pos);
        k.checked_sub(1).map(|k| self.group_label(k))
    }

    /// "Agrupar por" from the View menu or the palette.
    pub fn set_group_by(&mut self, by: GroupBy, cx: &mut Context<Self>) {
        if self.groups.by == by {
            return;
        }
        self.groups.by = by;
        self.groups.headers.clear();
        self.groups.at.clear();
        self.groups.version = self.groups.rows_version.wrapping_sub(1);
        self.resort(cx);
        cx.notify();
    }

    /// Finds the headers again when the rows changed (from render).
    pub(super) fn refresh_groups(&mut self, cx: &mut Context<Self>) {
        let g = &mut self.groups;
        if g.by == GroupBy::None || g.running || g.version == g.rows_version {
            return;
        }
        g.running = true;
        let (by, version) = (g.by, g.rows_version);
        let rows = self.rows().to_vec();
        let list = self.list.clone();
        let now = now();
        cx.spawn(async move |this, cx| {
            let headers = cx
                .background_spawn(
                    async move { grouping::headers(&rows, by, now, |i| facts(&list, i)) },
                )
                .await;
            let _ = this.update(cx, |view, cx| {
                let g = &mut view.groups;
                g.running = false;
                if g.by != by {
                    return;
                }
                g.at = headers
                    .iter()
                    .enumerate()
                    .map(|(k, (p, _))| p + k)
                    .collect();
                g.headers = headers;
                g.version = version;
                cx.notify();
            });
        })
        .detach();
    }

    /// Shows or hides a media column (View menu, palette).
    pub fn toggle_media_column(&mut self, col: MediaCol, cx: &mut Context<Self>) {
        let k = col.index();
        self.media.shown[k] = !self.media.shown[k];
        cx.notify();
    }

    pub(super) fn media_value(&self, i: u32, col: MediaCol) -> Option<SharedString> {
        self.media.values.get(self.list.name(i))?[col.index()].clone()
    }

    /// Asks for the values of the files drawn that have none yet.
    pub(super) fn want_media(&mut self, entries: &[u32], cx: &mut Context<Self>) {
        if !self.media.any() {
            return;
        }
        if self.media.dir != self.path {
            self.media.dir = self.path.clone();
            self.media.values.clear();
            self.media.asked.clear();
            self.media.wanted.clear();
        }
        for &i in entries {
            if self.list.attributes(i).is_dir() {
                continue;
            }
            let name = self.list.name(i);
            if !self.media.values.contains_key(name) && !self.media.asked.contains(name) {
                self.media.asked.insert(name.to_owned());
                self.media.wanted.push(name.to_owned());
            }
        }
        self.read_media(cx);
    }

    /// Reads the wanted values on a thread, a batch at a time.
    fn read_media(&mut self, cx: &mut Context<Self>) {
        if self.media.running || self.media.wanted.is_empty() {
            return;
        }
        self.media.running = true;
        let names = std::mem::take(&mut self.media.wanted);
        let dir = self.media.dir.clone();
        let (tx, mut rx) = futures::channel::mpsc::unbounded::<Vec<(String, Values)>>();
        let folder = dir.clone();
        let _ = std::thread::Builder::new()
            .name("fx-media".into())
            .spawn(move || {
                let props: Vec<&str> = MediaCol::ALL.iter().map(|c| c.property()).collect();
                for chunk in names.chunks(16) {
                    let read: Vec<(String, Values)> = chunk
                        .iter()
                        .map(|name| {
                            let values = fx_shell::property_values(&folder.join(name), &props)
                                .unwrap_or_default();
                            let mut out: Values = Default::default();
                            for (k, v) in values.into_iter().take(3).enumerate() {
                                out[k] = v.map(SharedString::from);
                            }
                            (name.clone(), out)
                        })
                        .collect();
                    if tx.unbounded_send(read).is_err() {
                        return;
                    }
                }
            });
        cx.spawn(async move |this, cx| {
            while let Some(read) = rx.next().await {
                let _ = this.update(cx, |view, cx| {
                    if view.media.dir == dir {
                        view.media.values.extend(read);
                        cx.notify();
                    }
                });
            }
            let _ = this.update(cx, |view, cx| {
                view.media.running = false;
                view.read_media(cx);
            });
        })
        .detach();
    }
}
