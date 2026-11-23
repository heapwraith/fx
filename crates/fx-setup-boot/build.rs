//! Windows resources of the setup launcher (T5.7): the fx icon (what the
//! downloaded installer shows), the version information and a manifest that
//! runs it as the user (a program named "setup" without one is elevated by
//! the system's installer detection).

use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=fx-setup-boot.manifest");
    println!("cargo:rerun-if-changed=../fx-ui/resources/fx.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return Ok(());
    }
    let dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR")?);
    let out = PathBuf::from(std::env::var("OUT_DIR")?);
    let version = std::env::var("CARGO_PKG_VERSION")?;
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
    // llvm-rc resolves relative paths from OUT_DIR: absolute paths.
    let abs = |p: &str| dir.join(p).display().to_string().replace('\\', "/");
    let rc = format!(
        r#"#pragma code_page(65001)
1 ICON "{icon}"
1 24 "{manifest}"

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
      VALUE "FileDescription", "Pathdash — instalador"
      VALUE "FileVersion", "{version}"
      VALUE "InternalName", "fx-setup"
      VALUE "LegalCopyright", "MIT OR Apache-2.0"
      VALUE "OriginalFilename", "fx-setup.exe"
      VALUE "ProductName", "Pathdash"
      VALUE "ProductVersion", "{version}"
    END
  END
  BLOCK "VarFileInfo"
  BEGIN
    VALUE "Translation", 0x0416, 1200
  END
END
"#,
        icon = abs("../fx-ui/resources/fx.ico"),
        manifest = abs("fx-setup-boot.manifest"),
    );
    let rc_path = out.join("boot.rc");
    std::fs::write(&rc_path, rc)?;
    embed_resource::compile(&rc_path, embed_resource::NONE).manifest_optional()?;
    Ok(())
}
