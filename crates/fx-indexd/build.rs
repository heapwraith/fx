//! Windows resources of `fx-indexd.exe`: the Pathdash icon and the version
//! information, so Task Manager, Services and the file's Properties show
//! "Pathdash — serviço de índice" and not just the file name (the file keeps its
//! name: installed versions and the service depend on it, D-074).

use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let icon = PathBuf::from("../fx-ui/resources/fx.ico");
    println!("cargo:rerun-if-changed={}", icon.display());
    println!("cargo:rerun-if-changed=build.rs");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return Ok(());
    }
    let version = std::env::var("CARGO_PKG_VERSION")?;
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
      VALUE "CompanyName", "Pathdash"
      VALUE "FileDescription", "Pathdash — serviço de índice"
      VALUE "FileVersion", "{version}"
      VALUE "InternalName", "fx-indexd"
      VALUE "LegalCopyright", "MIT OR Apache-2.0"
      VALUE "OriginalFilename", "fx-indexd.exe"
      VALUE "ProductName", "Pathdash"
      VALUE "ProductVersion", "{version}"
    END
  END
  BLOCK "VarFileInfo"
  BEGIN
    VALUE "Translation", 0x0416, 1200
  END
END
"#
    );
    let rc_path = out.join("fx-indexd.rc");
    std::fs::write(&rc_path, rc)?;
    embed_resource::compile(&rc_path, embed_resource::NONE).manifest_optional()?;
    Ok(())
}
