//! Comparing files (T6.17): the lines of two texts side by side, by the
//! Myers algorithm (the shortest edit script, as in git and diff), after
//! the common start and end are set aside. Edits past `MAX_EDITS` are not
//! worked out line by line: the caller says the texts are too different.

/// Edits worked out at most (memory grows with their square).
pub const MAX_EDITS: usize = 2_000;

/// The text of a file: UTF-8 (BOM or not) or UTF-16 with a BOM. `None` for
/// binary content (a NUL byte in the first 8 KiB, outside UTF-16).
pub fn decode(bytes: &[u8]) -> Option<String> {
    let utf16 = |b: &[u8], read: fn([u8; 2]) -> u16| {
        let units: Vec<u16> = b.as_chunks::<2>().0.iter().map(|c| read(*c)).collect();
        String::from_utf16_lossy(&units)
    };
    if let Some(rest) = bytes.strip_prefix(&[0xFF, 0xFE]) {
        return Some(utf16(rest, u16::from_le_bytes));
    }
    if let Some(rest) = bytes.strip_prefix(&[0xFE, 0xFF]) {
        return Some(utf16(rest, u16::from_be_bytes));
    }
    if bytes[..bytes.len().min(8192)].contains(&0) {
        return None;
    }
    let rest = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(bytes);
    Some(String::from_utf8_lossy(rest).into_owned())
}

/// The lines of a text (CR LF or LF; a final newline adds no line).
pub fn lines(text: &str) -> Vec<&str> {
    let mut v: Vec<&str> = text
        .split('\n')
        .map(|l| l.strip_suffix('\r').unwrap_or(l))
        .collect();
    if v.len() > 1 && v.last().is_some_and(|l| l.is_empty()) {
        v.pop();
    }
    if v == [""] {
        v.clear();
    }
    v
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Op {
    /// The same line: (left index, right index).
    Same(usize, usize),
    /// Only on the left.
    Removed(usize),
    /// Only on the right.
    Added(usize),
}

/// The edit script from `a` to `b`, or `None` past `max_edits` edits.
pub fn diff<T: PartialEq>(a: &[T], b: &[T], max_edits: usize) -> Option<Vec<Op>> {
    let prefix = a.iter().zip(b).take_while(|(x, y)| x == y).count();
    let suffix = a[prefix..]
        .iter()
        .rev()
        .zip(b[prefix..].iter().rev())
        .take_while(|(x, y)| x == y)
        .count();
    let (ma, mb) = (&a[prefix..a.len() - suffix], &b[prefix..b.len() - suffix]);
    let middle = myers(ma, mb, max_edits)?;
    let mut out: Vec<Op> = (0..prefix).map(|i| Op::Same(i, i)).collect();
    out.extend(middle.into_iter().map(|op| match op {
        Op::Same(i, j) => Op::Same(i + prefix, j + prefix),
        Op::Removed(i) => Op::Removed(i + prefix),
        Op::Added(j) => Op::Added(j + prefix),
    }));
    let (sa, sb) = (a.len() - suffix, b.len() - suffix);
    out.extend((0..suffix).map(|k| Op::Same(sa + k, sb + k)));
    Some(out)
}

fn myers<T: PartialEq>(a: &[T], b: &[T], max_edits: usize) -> Option<Vec<Op>> {
    let (n, m) = (a.len() as i64, b.len() as i64);
    let max = (n + m) as usize;
    let offset = max as i64 + 1;
    let mut v = vec![0i64; 2 * max + 3];
    // `trace[d]`: v for k in -(d+1)..=(d+1), before round d.
    let mut trace: Vec<Vec<i64>> = Vec::new();
    for d in 0..=max as i64 {
        if d as usize > max_edits {
            return None;
        }
        let lo = (offset - d - 1) as usize;
        let hi = (offset + d + 1) as usize;
        trace.push(v[lo..=hi].to_vec());
        let mut k = -d;
        while k <= d {
            let at = |k: i64| (offset + k) as usize;
            let mut x = if k == -d || (k != d && v[at(k - 1)] < v[at(k + 1)]) {
                v[at(k + 1)]
            } else {
                v[at(k - 1)] + 1
            };
            let mut y = x - k;
            while x < n && y < m && a[x as usize] == b[y as usize] {
                x += 1;
                y += 1;
            }
            v[at(k)] = x;
            if x >= n && y >= m {
                return Some(backtrack(&trace, n, m));
            }
            k += 2;
        }
    }
    Some(Vec::new())
}

fn backtrack(trace: &[Vec<i64>], n: i64, m: i64) -> Vec<Op> {
    let (mut x, mut y) = (n, m);
    let mut out = Vec::new();
    for (d, snap) in trace.iter().enumerate().rev() {
        let d = d as i64;
        let get = |k: i64| snap[(k + d + 1) as usize];
        let k = x - y;
        let prev_k = if k == -d || (k != d && get(k - 1) < get(k + 1)) {
            k + 1
        } else {
            k - 1
        };
        let prev_x = get(prev_k);
        let prev_y = prev_x - prev_k;
        while x > prev_x && y > prev_y {
            out.push(Op::Same((x - 1) as usize, (y - 1) as usize));
            x -= 1;
            y -= 1;
        }
        if d > 0 {
            if x == prev_x {
                out.push(Op::Added((y - 1) as usize));
            } else {
                out.push(Op::Removed((x - 1) as usize));
            }
        }
        x = prev_x;
        y = prev_y;
    }
    out.reverse();
    out
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowKind {
    Same,
    /// A left line replaced by a right line.
    Changed,
    Removed,
    Added,
}

/// One line of the side-by-side view.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Row {
    pub left: Option<usize>,
    pub right: Option<usize>,
    pub kind: RowKind,
}

/// Rows for two columns: in each run of edits, removed and added lines
/// are paired as changed lines, the rest stay removed or added.
pub fn side_by_side(ops: &[Op]) -> Vec<Row> {
    let mut rows = Vec::with_capacity(ops.len());
    let mut i = 0;
    while i < ops.len() {
        if let Op::Same(l, r) = ops[i] {
            rows.push(Row {
                left: Some(l),
                right: Some(r),
                kind: RowKind::Same,
            });
            i += 1;
            continue;
        }
        let (mut removed, mut added) = (Vec::new(), Vec::new());
        while let Some(op) = ops.get(i) {
            match *op {
                Op::Removed(l) => removed.push(l),
                Op::Added(r) => added.push(r),
                Op::Same(..) => break,
            }
            i += 1;
        }
        for k in 0..removed.len().max(added.len()) {
            let (left, right) = (removed.get(k).copied(), added.get(k).copied());
            let kind = match (left, right) {
                (Some(_), Some(_)) => RowKind::Changed,
                (Some(_), None) => RowKind::Removed,
                _ => RowKind::Added,
            };
            rows.push(Row { left, right, kind });
        }
    }
    rows
}

/// Where each block of differences starts (row indices).
pub fn change_starts(rows: &[Row]) -> Vec<usize> {
    (0..rows.len())
        .filter(|&k| rows[k].kind != RowKind::Same && (k == 0 || rows[k - 1].kind == RowKind::Same))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn apply(a: &[&str], b: &[&str], ops: &[Op]) -> (Vec<String>, Vec<String>) {
        let mut left = Vec::new();
        let mut right = Vec::new();
        for op in ops {
            match *op {
                Op::Same(i, j) => {
                    assert_eq!(a[i], b[j]);
                    left.push(a[i].to_owned());
                    right.push(b[j].to_owned());
                }
                Op::Removed(i) => left.push(a[i].to_owned()),
                Op::Added(j) => right.push(b[j].to_owned()),
            }
        }
        (left, right)
    }

    #[test]
    fn shortest_script_rebuilds_both_sides() {
        let a = lines("a\nb\nc\nd\ne\nf\n");
        let b = lines("a\nx\nc\nd\nf\ng\n");
        let ops = diff(&a, &b, MAX_EDITS).unwrap();
        let edits = ops.iter().filter(|o| !matches!(o, Op::Same(..))).count();
        assert_eq!(edits, 4, "b→x (2), e removed, g added: {ops:?}");
        let (l, r) = apply(&a, &b, &ops);
        assert_eq!(l, a);
        assert_eq!(r, b);
        let rows = side_by_side(&ops);
        assert_eq!(
            rows.iter().map(|r| r.kind).collect::<Vec<_>>(),
            [
                RowKind::Same,
                RowKind::Changed,
                RowKind::Same,
                RowKind::Same,
                RowKind::Removed,
                RowKind::Same,
                RowKind::Added,
            ]
        );
        assert_eq!(change_starts(&rows), [1, 4, 6]);
    }

    #[test]
    fn edge_cases_and_limits() {
        let empty: Vec<&str> = Vec::new();
        let some = lines("1\n2");
        assert_eq!(diff(&empty, &empty, 10).unwrap(), []);
        assert_eq!(
            diff(&empty, &some, 10).unwrap(),
            [Op::Added(0), Op::Added(1)]
        );
        assert_eq!(
            diff(&some, &some, 0).unwrap(),
            [Op::Same(0, 0), Op::Same(1, 1)]
        );
        let a: Vec<u32> = (0..100).collect();
        let b: Vec<u32> = (100..200).collect();
        assert!(diff(&a, &b, 50).is_none(), "too different");
        assert_eq!(lines("x\r\ny\r\n"), ["x", "y"]);
        assert_eq!(decode(b"\xEF\xBB\xBFol\xC3\xA1").as_deref(), Some("olá"));
        assert_eq!(
            decode(&[0xFF, 0xFE, b'o', 0, b'i', 0]).as_deref(),
            Some("oi")
        );
        assert_eq!(decode(b"PK\x03\x04\0\0"), None);
    }

    #[test]
    fn ten_thousand_lines_with_scattered_edits() {
        let a: Vec<String> = (0..10_000).map(|k| format!("linha {k}")).collect();
        let mut b = a.clone();
        for k in (0..10_000).step_by(500) {
            b[k] = format!("mudou {k}");
        }
        b.insert(1234, "nova".into());
        let started = std::time::Instant::now();
        let ops = diff(&a, &b, MAX_EDITS).unwrap();
        println!("10k lines, 41 edits: {:?}", started.elapsed());
        let edits = ops.iter().filter(|o| !matches!(o, Op::Same(..))).count();
        assert_eq!(edits, 41);
    }
}
