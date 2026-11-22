//! What fx was asked to open from the command line. With fx as the default
//! file manager, Windows hands it every folder, including the ones that are
//! not on disk (Recycle Bin, This PC, Control Panel), named `::{CLSID}`;
//! fx gives those back to Explorer instead of failing on a path that does not
//! exist (review, D-066; reported by users of Files and Directory Opus).

use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};

/// The command-line argument, sorted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShellArgument {
    /// A folder or file on disk: fx opens it.
    Path(PathBuf),
    /// A shell namespace name fx does not show (`::{…}`, `shell:…`): for
    /// Explorer.
    Virtual(OsString),
}

/// Sorts `arg`. A `::{CLSID}\…` name that ends in a disk path (This PC ›
/// `C:\Users`) is that path.
pub fn classify(arg: &OsStr) -> ShellArgument {
    let Some(text) = arg.to_str() else {
        // Not Unicode: only a real path can be like that.
        return ShellArgument::Path(PathBuf::from(arg));
    };
    let virtual_name = || ShellArgument::Virtual(arg.to_owned());
    if text
        .get(..6)
        .is_some_and(|p| p.eq_ignore_ascii_case("shell:"))
    {
        return virtual_name();
    }
    if !text.starts_with("::") {
        return ShellArgument::Path(PathBuf::from(arg));
    }
    // Skip the leading shell folders; what is left may be a disk path.
    let rest: Vec<&str> = text
        .split('\\')
        .skip_while(|part| part.starts_with("::{"))
        .collect();
    let on_disk = rest.join("\\");
    let path = Path::new(&on_disk);
    if rest.first().is_some_and(|first| is_drive(first)) && path.is_absolute() {
        ShellArgument::Path(path.to_path_buf())
    } else if rest.first().is_some_and(|first| is_drive(first)) {
        // `C:` alone: the drive's root.
        ShellArgument::Path(PathBuf::from(format!("{on_disk}\\")))
    } else {
        virtual_name()
    }
}

/// `C:`, `d:`.
fn is_drive(part: &str) -> bool {
    let b = part.as_bytes();
    b.len() == 2 && b[0].is_ascii_alphabetic() && b[1] == b':'
}

#[cfg(test)]
mod tests {
    use super::*;

    fn path(p: &str) -> ShellArgument {
        ShellArgument::Path(PathBuf::from(p))
    }

    fn virt(p: &str) -> ShellArgument {
        ShellArgument::Virtual(OsString::from(p))
    }

    #[test]
    fn disk_paths_stay_paths() {
        assert_eq!(classify(OsStr::new(r"C:\Users\ana")), path(r"C:\Users\ana"));
        assert_eq!(
            classify(OsStr::new(r"\\servidor\pasta")),
            path(r"\\servidor\pasta")
        );
        assert_eq!(classify(OsStr::new(".")), path("."));
    }

    #[test]
    fn shell_folders_go_to_explorer() {
        let bin = "::{645FF040-5081-101B-9F08-00AA002F954E}";
        assert_eq!(classify(OsStr::new(bin)), virt(bin));
        let control = r"::{26EE0668-A00A-44D7-9371-BEB064C98683}\0";
        assert_eq!(classify(OsStr::new(control)), virt(control));
        assert_eq!(
            classify(OsStr::new("shell:RecycleBinFolder")),
            virt("shell:RecycleBinFolder")
        );
        assert_eq!(
            classify(OsStr::new("SHELL:Downloads")),
            virt("SHELL:Downloads")
        );
    }

    #[test]
    fn this_pc_drives_and_folders_are_paths() {
        let pc = "::{20D04FE0-3AEA-1069-A2D8-08002B30309D}";
        assert_eq!(classify(OsStr::new(&format!(r"{pc}\C:\"))), path(r"C:\"));
        assert_eq!(classify(OsStr::new(&format!(r"{pc}\D:"))), path(r"D:\"));
        assert_eq!(
            classify(OsStr::new(&format!(r"{pc}\C:\Users\ana"))),
            path(r"C:\Users\ana")
        );
        assert_eq!(classify(OsStr::new(pc)), virt(pc), "This PC itself");
    }
}
