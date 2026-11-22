//! Windows resources of `fx.exe` (task T2.1): the app icon (id 1, which GPUI
//! loads for the window class, so the title bar, taskbar and Alt+Tab show it)
//! and the version information shown in the file's Properties. GPUI already
//! embeds the manifest (DPI awareness, Common Controls 6).

use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let icon = PathBuf::from("resources/fx.ico");
    println!("cargo:rerun-if-changed={}", icon.display());
    println!("cargo:rerun-if-changed=build.rs");
    let version = std::env::var("CARGO_PKG_VERSION")?;
    let full = format!("{version}+{}", commit());
    println!("cargo:rustc-env=FX_VERSION={full}");
    println!("cargo:rerun-if-changed=../../.git/HEAD");
    println!("cargo:rerun-if-changed=../../.git/index");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return Ok(());
    }
    // The updater's DLLs (WinHTTP, WinTrust and what they pull in, BCrypt)
    // load only when an update check runs, not at every start: they cost
    // about 1 MB of P8 (T4.5).
    for dll in ["winhttp.dll", "wintrust.dll", "bcrypt.dll"] {
        println!("cargo:rustc-link-arg-bins=/DELAYLOAD:{dll}");
    }
    println!("cargo:rustc-link-arg-bins=delayimp.lib");
    let dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR")?);
    let out = PathBuf::from(std::env::var("OUT_DIR")?);
    let mut parts: Vec<u16> = version
        .split(['.', '-'])
        .filter_map(|p| p.parse().ok())
        .take(3)
        .collect();
    parts.resize(4, 0);
    let numeric = parts
        .iter()
        .map(u16::to_string)
        .collect::<Vec<_>>()
        .join(",");
    // llvm-rc resolves relative paths from OUT_DIR, so the icon path is absolute.
    let icon_abs = dir.join(&icon).display().to_string().replace('\\', "/");
    let rc = format!(
        r#"#pragma code_page(65001)
1 ICON "{icon_abs}"

1 VERSIONINFO
FILEVERSION {numeric}
PRODUCTVERSION {numeric}
FILEOS 0x40004
FILETYPE 0x1
BEGIN
  BLOCK "StringFileInfo"
  BEGIN
    BLOCK "041604B0"
    BEGIN
      VALUE "CompanyName", "fx"
      VALUE "FileDescription", "fx — gerenciador de arquivos"
      VALUE "FileVersion", "{version}"
      VALUE "InternalName", "fx"
      VALUE "LegalCopyright", "MIT OR Apache-2.0"
      VALUE "OriginalFilename", "fx.exe"
      VALUE "ProductName", "fx"
      VALUE "ProductVersion", "{full}"
    END
  END
  BLOCK "VarFileInfo"
  BEGIN
    VALUE "Translation", 0x0416, 1200
  END
END
"#
    );
    let rc_path = out.join("fx.rc");
    std::fs::write(&rc_path, rc)?;
    embed_resource::compile(&rc_path, embed_resource::NONE).manifest_optional()?;
    Ok(())
}

/// Short commit hash, with `-dirty` when the tree has local changes;
/// `unknown` outside a git checkout (e.g. a source tarball).
fn commit() -> String {
    let git = |args: &[&str]| {
        std::process::Command::new("git")
            .args(args)
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
    };
    let Some(mut hash) = git(&["rev-parse", "--short=7", "HEAD"]) else {
        return "unknown".to_owned();
    };
    if git(&["status", "--porcelain"]).is_some_and(|s| !s.is_empty()) {
        hash.push_str("-dirty");
    }
    hash
}
