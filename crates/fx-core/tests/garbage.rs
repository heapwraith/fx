//! Every reader of outside data must survive garbage: random bytes and
//! damaged copies of valid input, many times over, without a panic (review,
//! D-066: the community's answer to "what if the file is wrong" is fuzzing;
//! this is its deterministic, dependency-free version, run with the tests).

use std::ffi::OsStr;

/// xorshift64*: a fixed seed, so a failure always comes back.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n.max(1) as u64) as usize
    }

    fn bytes(&mut self, max: usize) -> Vec<u8> {
        let len = self.below(max + 1);
        (0..len).map(|_| self.next() as u8).collect()
    }

    /// `valid` with a few bytes flipped, cut short or extended.
    fn damaged(&mut self, valid: &[u8]) -> Vec<u8> {
        let mut v = valid.to_vec();
        match self.below(3) {
            0 => {
                for _ in 0..=self.below(4) {
                    if !v.is_empty() {
                        let at = self.below(v.len());
                        v[at] = self.next() as u8;
                    }
                }
            }
            1 => v.truncate(self.below(v.len() + 1)),
            _ => v.extend(self.bytes(16)),
        }
        v
    }
}

const ROUNDS: usize = 3000;

#[test]
fn packages() {
    use fx_core::package::{Manifest, decode, detach};
    let mut rng = Rng(0x00F0_F0F0_1234_5678);
    // A valid payload to damage.
    let manifest = Manifest {
        version: "1.2.3".into(),
        files: vec![fx_core::package::FileEntry {
            name: "fx.exe".into(),
            size: 3,
            sha256: [7; 32],
        }],
    };
    let valid = fx_core::package::encode(&manifest, b"abc");
    for _ in 0..ROUNDS {
        let _ = decode(&rng.damaged(&valid));
        let _ = decode(&rng.bytes(256));
        let _ = detach(&rng.bytes(64));
    }
}

#[test]
fn update_answers_and_notes() {
    use fx_core::update::{Version, expected_hash, json, parse_ready_note, pick};
    let mut rng = Rng(0xDEAD_BEEF_0000_0001);
    let valid = br#"[{"tag_name":"v1.2.3","draft":false,"prerelease":false,"assets":[{"name":"fx-1.2.3-setup-x64.exe","browser_download_url":"https://x/y"},{"name":"SHA256SUMS.txt","browser_download_url":"https://x/s"}]}]"#;
    let current = Version::parse("1.0.0").unwrap();
    for _ in 0..ROUNDS {
        let text = String::from_utf8_lossy(&rng.damaged(valid)).into_owned();
        let _ = json::parse(&text);
        let _ = pick(&text, &current, "x64");
        let noise = String::from_utf8_lossy(&rng.bytes(128)).into_owned();
        let _ = json::parse(&noise);
        let _ = Version::parse(&noise);
        let _ = expected_hash(&noise, "fx.exe");
        let _ = parse_ready_note(&noise);
    }
    // Deep nesting must not overflow the stack.
    let deep = "[".repeat(100_000);
    let _ = json::parse(&deep);
}

#[test]
fn terminal_output() {
    let mut rng = Rng(0x1357_9BDF_2468_ACE0);
    let mut screen = fx_core::vt::Screen::new(80, 25);
    for _ in 0..ROUNDS {
        let mut chunk = rng.bytes(64);
        // Lots of escape sequences.
        if rng.below(2) == 0 {
            chunk.insert(0, 0x1B);
            chunk.insert(1, b'[');
        }
        screen.feed(&chunk);
    }
    // Absurd parameters.
    screen.feed(b"\x1b[999999999;999999999H\x1b[99999999999999999999J\x1b[38;5;999m");
}

#[test]
fn text_readers() {
    let mut rng = Rng(0xCAFE_F00D_0000_0042);
    for _ in 0..ROUNDS {
        let text = String::from_utf8_lossy(&rng.bytes(96)).into_owned();
        let _ = fx_core::crash::scrub(&text, Some("ana"));
        let _ = fx_core::shell_arg::classify(OsStr::new(&text));
        let _ = fx_core::temp_name::is_temp_name(&text);
        let _ = fx_core::search_filter::Filters::parse(&text, 0, 0);
        let _ = fx_core::markdown::render(&[text.as_str()]);
        if let Some(lang) = fx_core::highlight::language_for("rs") {
            let _ = fx_core::highlight::highlight(&[text.as_str()], lang);
        }
        let _ = fx_core::zip_list::parse_central(&rng.bytes(128), 1 << 20, 100);
    }
}
