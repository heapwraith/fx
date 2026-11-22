//! Building the setup file (`fx-setup --pack`, run by the CI) and reading
//! the package inside the running setup program (`fx_core::package`).

use std::io;
use std::path::Path;

use fx_core::package::{self, FileEntry, Manifest};

/// The setup program itself, installed as the uninstaller and the update
/// helper.
pub const SETUP_EXE: &str = "fx-setup.exe";
/// Programs taken from the build folder.
pub const PROGRAMS: [&str; 3] = ["fx.exe", "fx-indexd.exe", "fx-preview.exe"];
/// Documents taken from the repository root.
pub const DOCUMENTS: [&str; 4] = ["LICENSE-MIT", "LICENSE-APACHE", "README.md", "CHANGELOG.md"];

/// A package read back: its list and the files.
#[derive(Debug)]
pub struct Package {
    pub manifest: Manifest,
    pub files: Vec<(String, Vec<u8>)>,
}

impl Package {
    pub fn version(&self) -> &str {
        &self.manifest.version
    }
}

fn hash(bytes: &[u8]) -> [u8; 32] {
    // SHA-256 cannot fail for an in-memory buffer; a zero hash makes the
    // check fail loudly if the system call ever did.
    fx_shell::net::sha256(bytes).unwrap_or([0; 32])
}

/// Writes `out`: the `stub` setup program with a package of `files`
/// (`(name, bytes)`, the stub itself is added as `fx-setup.exe`).
pub fn build(stub: &[u8], version: &str, files: &[(String, Vec<u8>)]) -> io::Result<Vec<u8>> {
    let mut all: Vec<(&str, &[u8])> = vec![(SETUP_EXE, stub)];
    all.extend(files.iter().map(|(n, b)| (n.as_str(), b.as_slice())));
    let mut data = Vec::new();
    let mut entries = Vec::new();
    for (name, bytes) in &all {
        if !package::is_plain_name(name) {
            return Err(io::Error::other(format!("unsafe file name: {name}")));
        }
        entries.push(FileEntry {
            name: (*name).to_owned(),
            size: bytes.len() as u64,
            sha256: hash(bytes),
        });
        data.extend_from_slice(bytes);
    }
    let blob = fx_shell::install::compress(&data)?;
    let manifest = Manifest {
        version: version.to_owned(),
        files: entries,
    };
    Ok(package::attach(stub, &package::encode(&manifest, &blob)))
}

/// `fx-setup --pack`: the programs of `bin_dir`, the documents of `doc_dir`
/// and `stub` (this program, or the ARM64 one) into `out`.
pub fn pack_dirs(
    stub: &Path,
    bin_dir: &Path,
    doc_dir: &Path,
    version: &str,
    out: &Path,
) -> io::Result<u64> {
    let stub_bytes = std::fs::read(stub)?;
    // A setup that already carries a package gives only its program.
    let stub_bytes = match package::detach(&stub_bytes) {
        Some((program, _)) => program.to_vec(),
        None => stub_bytes,
    };
    let mut files = Vec::new();
    for name in PROGRAMS {
        files.push((name.to_owned(), std::fs::read(bin_dir.join(name))?));
    }
    for name in DOCUMENTS {
        files.push((name.to_owned(), std::fs::read(doc_dir.join(name))?));
    }
    let setup = build(&stub_bytes, version, &files)?;
    std::fs::write(out, &setup)?;
    Ok(setup.len() as u64)
}

/// Reads the package carried by a setup file (`None`: a setup program
/// without one, such as the installed uninstaller).
pub fn read(setup: &[u8]) -> io::Result<Option<Package>> {
    let Some((_, payload)) = package::detach(setup) else {
        return Ok(None);
    };
    let bad = |e: String| io::Error::new(io::ErrorKind::InvalidData, e);
    let (manifest, blob) = package::decode(payload).map_err(bad)?;
    let data = fx_shell::install::decompress(blob, manifest.total_size() as usize)?;
    let files = package::split(&manifest, &data, hash)
        .map_err(bad)?
        .into_iter()
        .map(|(n, b)| (n.to_owned(), b.to_vec()))
        .collect();
    Ok(Some(Package { manifest, files }))
}

/// The package of the running setup program.
pub fn read_self() -> io::Result<Option<Package>> {
    read(&std::fs::read(std::env::current_exe()?)?)
}

/// Just the program part of the running setup (to copy it elsewhere).
pub fn program_of(setup: &[u8]) -> &[u8] {
    package::detach(setup).map_or(setup, |(program, _)| program)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_and_read_back() {
        let files = vec![
            ("fx.exe".to_owned(), b"MZ fx program".repeat(1000)),
            ("README.md".to_owned(), "# fx\nação".as_bytes().to_vec()),
        ];
        let setup = build(b"MZ setup", "0.3.0", &files).unwrap();
        assert!(
            setup.len() < 13_000 + 8 + 400,
            "compressed: {}",
            setup.len()
        );
        let pkg = read(&setup).unwrap().unwrap();
        assert_eq!(pkg.version(), "0.3.0");
        assert_eq!(pkg.files[0], (SETUP_EXE.to_owned(), b"MZ setup".to_vec()));
        assert_eq!(pkg.files[1], files[0]);
        assert_eq!(pkg.files[2], files[1]);
        assert_eq!(program_of(&setup), b"MZ setup");
        assert!(read(b"MZ setup").unwrap().is_none());
    }

    #[test]
    fn a_damaged_setup_is_refused() {
        let files = vec![("fx.exe".to_owned(), vec![7u8; 5000])];
        let mut setup = build(b"MZ", "1.0.0", &files).unwrap();
        // Flip a byte in the compressed blob (before the 24-byte footer).
        let n = setup.len();
        setup[n - 30] ^= 0x55;
        assert!(read(&setup).is_err());
        assert!(build(b"MZ", "1", &[("..\\x".to_owned(), vec![])]).is_err());
    }

    #[test]
    fn pack_from_folders() {
        let dir = tempfile::tempdir().unwrap();
        let (bin, doc) = (dir.path().join("bin"), dir.path().join("doc"));
        std::fs::create_dir_all(&bin).unwrap();
        std::fs::create_dir_all(&doc).unwrap();
        for n in PROGRAMS {
            std::fs::write(bin.join(n), n.as_bytes()).unwrap();
        }
        for n in DOCUMENTS {
            std::fs::write(doc.join(n), n.as_bytes()).unwrap();
        }
        let stub = dir.path().join("stub.exe");
        std::fs::write(&stub, b"STUB").unwrap();
        let out = dir.path().join("setup.exe");
        pack_dirs(&stub, &bin, &doc, "9.9.9", &out).unwrap();
        let pkg = read(&std::fs::read(&out).unwrap()).unwrap().unwrap();
        assert_eq!(pkg.files.len(), 1 + PROGRAMS.len() + DOCUMENTS.len());
        // Packing from an already packed setup keeps only its program.
        let again = dir.path().join("again.exe");
        pack_dirs(&out, &bin, &doc, "9.9.9", &again).unwrap();
        let pkg = read(&std::fs::read(&again).unwrap()).unwrap().unwrap();
        assert_eq!(pkg.files[0].1, b"STUB");
    }
}
