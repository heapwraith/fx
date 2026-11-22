//! Updater logic (task T4.1), pure: version comparison, reading the GitHub
//! releases JSON, choosing the installer and checking it against the
//! release's `SHA256SUMS.txt`. Network, hashing and running the installer
//! are in fx-shell / fx-ui.

use std::cmp::Ordering;

/// A semantic version: `1.2.3`, `0.2.0-beta`, `v0.2.0-beta.2+abc` (the
/// `v` prefix and the build metadata after `+` are ignored).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Version {
    pub major: u64,
    pub minor: u64,
    pub patch: u64,
    /// Pre-release identifiers ("beta", "2"); empty for a release.
    pub pre: Vec<String>,
}

impl Version {
    pub fn parse(text: &str) -> Option<Version> {
        let text = text.trim().trim_start_matches(['v', 'V']);
        let text = text.split('+').next()?;
        let (core, pre) = match text.split_once('-') {
            Some((c, p)) => (c, p.split('.').map(str::to_owned).collect()),
            None => (text, Vec::new()),
        };
        let mut nums = core.split('.').map(|n| n.parse::<u64>().ok());
        let v = Version {
            major: nums.next()??,
            minor: nums.next()??,
            patch: nums.next()??,
            pre,
        };
        if nums.next().is_some() || v.pre.iter().any(String::is_empty) {
            return None;
        }
        Some(v)
    }
}

impl Ord for Version {
    fn cmp(&self, other: &Self) -> Ordering {
        (self.major, self.minor, self.patch)
            .cmp(&(other.major, other.minor, other.patch))
            .then_with(|| match (self.pre.is_empty(), other.pre.is_empty()) {
                (true, true) => Ordering::Equal,
                // A pre-release comes before its release.
                (true, false) => Ordering::Greater,
                (false, true) => Ordering::Less,
                (false, false) => {
                    for (a, b) in self.pre.iter().zip(&other.pre) {
                        let o = match (a.parse::<u64>(), b.parse::<u64>()) {
                            (Ok(x), Ok(y)) => x.cmp(&y),
                            (Ok(_), Err(_)) => Ordering::Less,
                            (Err(_), Ok(_)) => Ordering::Greater,
                            (Err(_), Err(_)) => a.cmp(b),
                        };
                        if o != Ordering::Equal {
                            return o;
                        }
                    }
                    self.pre.len().cmp(&other.pre.len())
                }
            })
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl std::fmt::Display for Version {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)?;
        if !self.pre.is_empty() {
            write!(f, "-{}", self.pre.join("."))?;
        }
        Ok(())
    }
}

/// A newer version and where to get it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Update {
    pub version: Version,
    pub setup_name: String,
    pub setup_url: String,
    pub sums_url: String,
}

pub const SUMS_FILE: &str = "SHA256SUMS.txt";

/// The newest published release above `current` that has the installer for
/// `arch` ("x64" or "arm64") and the checksum file. `json` is the answer of
/// `GET /repos/{owner}/{repo}/releases`; drafts are skipped.
pub fn pick(json: &str, current: &Version, arch: &str) -> Option<Update> {
    let value = json::parse(json)?;
    let suffix = format!("-setup-{arch}.exe");
    let mut best: Option<Update> = None;
    for release in value.as_array()? {
        if release.get("draft").and_then(json::Value::as_bool) == Some(true) {
            continue;
        }
        let Some(version) = release
            .get("tag_name")
            .and_then(json::Value::as_str)
            .and_then(Version::parse)
        else {
            continue;
        };
        if version <= *current || best.as_ref().is_some_and(|b| version <= b.version) {
            continue;
        }
        let assets = release.get("assets").and_then(json::Value::as_array);
        let find = |pred: &dyn Fn(&str) -> bool| {
            assets?.iter().find_map(|a| {
                let name = a.get("name")?.as_str()?;
                let url = a.get("browser_download_url")?.as_str()?;
                pred(name).then(|| (name.to_owned(), url.to_owned()))
            })
        };
        let (Some((setup_name, setup_url)), Some((_, sums_url))) = (
            find(&|n: &str| n.ends_with(&suffix)),
            find(&|n: &str| n == SUMS_FILE),
        ) else {
            continue;
        };
        best = Some(Update {
            version,
            setup_name,
            setup_url,
            sums_url,
        });
    }
    best
}

/// The expected SHA-256 of `file` in a `sha256sum`-style list
/// (`<64 hex>  <name>` or `<64 hex> *<name>`).
pub fn expected_hash(sums: &str, file: &str) -> Option<[u8; 32]> {
    sums.lines().find_map(|line| {
        let (hex, name) = line.trim().split_once(char::is_whitespace)?;
        let name = name.trim().trim_start_matches('*');
        (name == file).then(|| decode_hex(hex))?
    })
}

fn decode_hex(hex: &str) -> Option<[u8; 32]> {
    if hex.len() != 64 {
        return None;
    }
    let mut out = [0u8; 32];
    for (k, byte) in out.iter_mut().enumerate() {
        *byte = u8::from_str_radix(hex.get(2 * k..2 * k + 2)?, 16).ok()?;
    }
    Some(out)
}

/// The note left next to a downloaded installer: version, file name and its
/// SHA-256, checked again right before it runs (review, D-066).
pub fn ready_note(version: &Version, setup_name: &str, hash: &[u8; 32]) -> String {
    let hex: String = hash.iter().map(|b| format!("{b:02x}")).collect();
    format!("{version}\n{setup_name}\n{hex}\n")
}

/// Reads [`ready_note`]; `None` for anything else (an old two-line note, a
/// damaged one, a name with folders).
pub fn parse_ready_note(text: &str) -> Option<(Version, String, [u8; 32])> {
    let mut lines = text.lines();
    let version = Version::parse(lines.next()?)?;
    let name = lines.next()?.trim().to_owned();
    let hash = decode_hex(lines.next()?.trim())?;
    crate::package::is_plain_name(&name).then_some((version, name, hash))
}

/// Whether a downloaded file is the published one.
pub fn verify(sums: &str, file: &str, actual: &[u8; 32]) -> bool {
    expected_hash(sums, file).is_some_and(|e| &e == actual)
}

/// Whether a check is due: at most one a day.
pub fn due(last_check_secs: Option<u64>, now_secs: u64) -> bool {
    last_check_secs.is_none_or(|t| now_secs.saturating_sub(t) >= 24 * 3600 || t > now_secs)
}

/// A minimal JSON reader: enough for the GitHub answer (objects, arrays,
/// strings with escapes, numbers, booleans, null).
pub mod json {
    #[derive(Debug, Clone, PartialEq)]
    pub enum Value {
        Null,
        Bool(bool),
        Number(f64),
        String(String),
        Array(Vec<Value>),
        Object(Vec<(String, Value)>),
    }

    impl Value {
        pub fn get(&self, key: &str) -> Option<&Value> {
            match self {
                Value::Object(fields) => fields.iter().find(|(k, _)| k == key).map(|(_, v)| v),
                _ => None,
            }
        }
        pub fn as_str(&self) -> Option<&str> {
            match self {
                Value::String(s) => Some(s),
                _ => None,
            }
        }
        pub fn as_bool(&self) -> Option<bool> {
            match self {
                Value::Bool(b) => Some(*b),
                _ => None,
            }
        }
        pub fn as_array(&self) -> Option<&[Value]> {
            match self {
                Value::Array(a) => Some(a),
                _ => None,
            }
        }
    }

    pub fn parse(text: &str) -> Option<Value> {
        let mut p = Parser {
            s: text.as_bytes(),
            i: 0,
            depth: 0,
        };
        let v = p.value()?;
        p.ws();
        (p.i == p.s.len()).then_some(v)
    }

    struct Parser<'a> {
        s: &'a [u8],
        i: usize,
        depth: u32,
    }

    impl Parser<'_> {
        fn ws(&mut self) {
            while self.i < self.s.len() && self.s[self.i].is_ascii_whitespace() {
                self.i += 1;
            }
        }

        fn eat(&mut self, b: u8) -> Option<()> {
            self.ws();
            (self.s.get(self.i) == Some(&b)).then(|| self.i += 1)
        }

        fn value(&mut self) -> Option<Value> {
            self.ws();
            self.depth += 1;
            if self.depth > 64 {
                return None;
            }
            let v = match *self.s.get(self.i)? {
                b'{' => {
                    self.i += 1;
                    let mut fields = Vec::new();
                    if self.eat(b'}').is_none() {
                        loop {
                            self.ws();
                            let k = self.string()?;
                            self.eat(b':')?;
                            fields.push((k, self.value()?));
                            if self.eat(b',').is_none() {
                                self.eat(b'}')?;
                                break;
                            }
                        }
                    }
                    Value::Object(fields)
                }
                b'[' => {
                    self.i += 1;
                    let mut items = Vec::new();
                    if self.eat(b']').is_none() {
                        loop {
                            items.push(self.value()?);
                            if self.eat(b',').is_none() {
                                self.eat(b']')?;
                                break;
                            }
                        }
                    }
                    Value::Array(items)
                }
                b'"' => Value::String(self.string()?),
                b't' => self.word("true", Value::Bool(true))?,
                b'f' => self.word("false", Value::Bool(false))?,
                b'n' => self.word("null", Value::Null)?,
                _ => {
                    let start = self.i;
                    while self.i < self.s.len()
                        && matches!(
                            self.s[self.i],
                            b'-' | b'+' | b'.' | b'e' | b'E' | b'0'..=b'9'
                        )
                    {
                        self.i += 1;
                    }
                    let n = std::str::from_utf8(&self.s[start..self.i]).ok()?;
                    Value::Number(n.parse().ok()?)
                }
            };
            self.depth -= 1;
            Some(v)
        }

        fn word(&mut self, w: &str, v: Value) -> Option<Value> {
            self.s[self.i..].starts_with(w.as_bytes()).then(|| {
                self.i += w.len();
                v
            })
        }

        fn string(&mut self) -> Option<String> {
            if self.s.get(self.i) != Some(&b'"') {
                return None;
            }
            self.i += 1;
            let mut out: Vec<u8> = Vec::new();
            loop {
                let b = *self.s.get(self.i)?;
                self.i += 1;
                match b {
                    b'"' => return String::from_utf8(out).ok(),
                    b'\\' => {
                        let e = *self.s.get(self.i)?;
                        self.i += 1;
                        let c = match e {
                            b'"' => '"',
                            b'\\' => '\\',
                            b'/' => '/',
                            b'b' => '\u{8}',
                            b'f' => '\u{c}',
                            b'n' => '\n',
                            b'r' => '\r',
                            b't' => '\t',
                            b'u' => {
                                let hi = self.hex4()?;
                                if (0xD800..0xDC00).contains(&hi) {
                                    // Surrogate pair.
                                    if self.s.get(self.i..self.i + 2)? != b"\\u" {
                                        return None;
                                    }
                                    self.i += 2;
                                    let lo = self.hex4()?;
                                    char::from_u32(
                                        0x10000
                                            + ((hi - 0xD800) << 10)
                                            + (lo.checked_sub(0xDC00)?),
                                    )?
                                } else {
                                    char::from_u32(hi)?
                                }
                            }
                            _ => return None,
                        };
                        let mut buf = [0u8; 4];
                        out.extend_from_slice(c.encode_utf8(&mut buf).as_bytes());
                    }
                    _ => out.push(b),
                }
            }
        }

        fn hex4(&mut self) -> Option<u32> {
            let h = std::str::from_utf8(self.s.get(self.i..self.i + 4)?).ok()?;
            self.i += 4;
            u32::from_str_radix(h, 16).ok()
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn ready_note_round_trips_and_rejects_the_rest() {
        let v = super::Version::parse("1.2.3").unwrap();
        let hash = [0xabu8; 32];
        let note = super::ready_note(&v, "fx-1.2.3-setup-x64.exe", &hash);
        assert_eq!(
            super::parse_ready_note(&note),
            Some((v.clone(), "fx-1.2.3-setup-x64.exe".to_owned(), hash))
        );
        assert_eq!(
            super::parse_ready_note("1.2.3\nfx-setup.exe\n"),
            None,
            "old note, no hash"
        );
        let folder = super::ready_note(&v, r"..\x.exe", &hash);
        assert_eq!(
            super::parse_ready_note(&folder),
            None,
            "folders are refused"
        );
        assert_eq!(super::parse_ready_note("1.2.3\nfx.exe\nzz\n"), None);
    }

    use super::*;

    fn v(s: &str) -> Version {
        Version::parse(s).unwrap()
    }

    #[test]
    fn versions_order_like_semver() {
        assert!(v("0.2.0-beta") < v("0.2.0"));
        assert!(v("0.2.0") < v("0.10.0"));
        assert!(v("0.2.0-alpha") < v("0.2.0-beta"));
        assert!(v("0.2.0-beta") < v("0.2.0-beta.2"));
        assert!(v("0.2.0-beta.2") < v("0.2.0-beta.10"));
        assert!(v("1.0.0-rc.1") < v("1.0.0"));
        assert_eq!(v("v0.2.0-beta+abc123-dirty"), v("0.2.0-beta"));
        assert_eq!(v("0.2.0-beta").to_string(), "0.2.0-beta");
        assert!(Version::parse("0.2").is_none());
        assert!(Version::parse("0.2.0.1").is_none());
        assert!(Version::parse("x.y.z").is_none());
    }

    const RELEASES: &str = r#"[
      {"tag_name": "v0.4.0", "draft": true, "assets": [
        {"name": "fx-0.4.0-setup-x64.exe", "browser_download_url": "https://e/0.4/setup"},
        {"name": "SHA256SUMS.txt", "browser_download_url": "https://e/0.4/sums"}]},
      {"tag_name": "v0.3.0-beta", "draft": false, "prerelease": true, "body": "linha \"citada\"\nç😀", "assets": [
        {"name": "fx-0.3.0-beta-setup-x64.exe", "browser_download_url": "https://e/0.3/setup", "size": 6000000},
        {"name": "fx-0.3.0-beta-setup-arm64.exe", "browser_download_url": "https://e/0.3/arm"},
        {"name": "SHA256SUMS.txt", "browser_download_url": "https://e/0.3/sums"}]},
      {"tag_name": "v0.2.5", "draft": false, "assets": [
        {"name": "fx-0.2.5-setup-x64.exe", "browser_download_url": "https://e/0.2.5/setup"}]},
      {"tag_name": "v0.2.0-beta", "draft": false, "assets": []}
    ]"#;

    #[test]
    fn picks_the_newest_published_release_with_installer_and_sums() {
        let u = pick(RELEASES, &v("0.2.0-beta"), "x64").unwrap();
        assert_eq!(u.version, v("0.3.0-beta"), "the draft 0.4.0 is skipped");
        assert_eq!(u.setup_url, "https://e/0.3/setup");
        assert_eq!(u.sums_url, "https://e/0.3/sums");
        assert_eq!(u.setup_name, "fx-0.3.0-beta-setup-x64.exe");
        assert_eq!(
            pick(RELEASES, &v("0.2.0-beta"), "arm64").unwrap().setup_url,
            "https://e/0.3/arm"
        );
        assert_eq!(
            pick(RELEASES, &v("0.3.0-beta"), "x64"),
            None,
            "already up to date"
        );
        assert_eq!(pick("not json", &v("0.1.0"), "x64"), None);
        assert_eq!(pick("{}", &v("0.1.0"), "x64"), None);
    }

    #[test]
    fn json_strings_and_values() {
        let j =
            json::parse(r#"{"a": "x\"y\\zç😀", "b": [1, -2.5e3, true, null], "c": {}}"#).unwrap();
        assert_eq!(j.get("a").unwrap().as_str(), Some("x\"y\\zç😀"));
        assert_eq!(j.get("b").unwrap().as_array().unwrap().len(), 4);
        assert!(json::parse(r#"{"a": }"#).is_none());
        assert!(json::parse("[1, 2").is_none());
        assert!(json::parse(&"[".repeat(1000)).is_none(), "depth limit");
    }

    #[test]
    fn hash_is_checked_against_the_sums_file() {
        let good = [0xABu8; 32];
        let hex: String = good.iter().map(|b| format!("{b:02x}")).collect();
        let sums = format!(
            "{}  fx-0.3.0-setup-x64.exe\n{hex} *fx-0.3.0-setup-arm64.exe\n{hex}  outro.zip\n",
            "0".repeat(64)
        );
        assert!(verify(&sums, "fx-0.3.0-setup-arm64.exe", &good));
        assert!(
            !verify(&sums, "fx-0.3.0-setup-x64.exe", &good),
            "altered file"
        );
        assert!(
            !verify(&sums, "fx-9.9.9-setup-x64.exe", &good),
            "missing from the list"
        );
        assert!(!verify("lixo", "outro.zip", &good));
    }

    #[test]
    fn once_a_day() {
        assert!(due(None, 1000));
        assert!(!due(Some(1000), 1000 + 3600));
        assert!(due(Some(1000), 1000 + 24 * 3600));
        assert!(due(Some(5000), 1000), "clock went back");
    }
}
