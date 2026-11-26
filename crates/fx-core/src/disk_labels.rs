//! What a folder or file on the disk *is* (D-078): the disk map names the
//! things that usually take the most space — games of each store, the WSL
//! and Docker disks, virtual machines, the Windows paging and hibernation
//! files, restore points, the Recycle Bin, temporary files — so the user
//! sees "Cyberpunk 2077 · jogo (Steam)" instead of a folder among many.
//! Only names and positions in the path are used; nothing is read.

use std::path::{Component, Path};

use crate::tr;

/// The group a recognized item belongs to, in the "what takes the most"
/// panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Category {
    Games,
    VirtualMachines,
    System,
    Cleanable,
}

impl Category {
    pub const ALL: [Category; 4] = [
        Category::Games,
        Category::VirtualMachines,
        Category::System,
        Category::Cleanable,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Category::Games => tr!("Jogos", "Games"),
            Category::VirtualMachines => {
                tr!("Máquinas virtuais e Linux", "Virtual machines and Linux")
            }
            Category::System => tr!("Sistema do Windows", "Windows system"),
            Category::Cleanable => tr!("Dá para limpar", "Can be cleaned up"),
        }
    }
}

/// A recognized item: its group and what to call it next to its name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Label {
    pub category: Category,
    /// "jogo (Steam)", "disco do WSL", "memória virtual do Windows".
    pub what: String,
}

fn label(category: Category, what: &str) -> Option<Label> {
    Some(Label {
        category,
        what: what.to_owned(),
    })
}

/// What `path` is, when it is one of the known space takers. `is_dir` tells
/// a folder from a file.
pub fn label_of(path: &Path, is_dir: bool) -> Option<Label> {
    let parts: Vec<String> = path
        .components()
        .filter_map(|c| match c {
            Component::Normal(n) => Some(n.to_string_lossy().to_lowercase()),
            _ => None,
        })
        .collect();
    let n = parts.len();
    if n == 0 {
        return None;
    }
    let last = parts[n - 1].as_str();
    let at = |k: usize| parts.get(n.wrapping_sub(k + 1)).map(String::as_str);
    let game = |store: &str| {
        label(
            Category::Games,
            &format!("{} ({store})", tr!("jogo", "game")),
        )
    };
    if is_dir {
        // Stores: the folder right below the library is one game.
        match at(1) {
            Some("common") if at(2) == Some("steamapps") => {
                if last.starts_with("steamworks") || last == "steam controller configs" {
                    return None;
                }
                return game("Steam");
            }
            Some("xboxgames") => return game("Xbox"),
            Some("epic games") if !last.starts_with("launcher") && last != "directxredist" => {
                return game("Epic");
            }
            Some("riot games") if !last.starts_with("riot client") => return game("Riot"),
            Some("gog games") | Some("gog galaxy") if last != "redists" => return game("GOG"),
            Some("ea games") => return game("EA"),
            Some("games") if at(2) == Some("ubisoft game launcher") => return game("Ubisoft"),
            Some("rockstar games")
                if !matches!(last, "launcher" | "social club" | "rockstar games launcher") =>
            {
                return game("Rockstar");
            }
            _ => {}
        }
        if n == 1 {
            match last {
                "$recycle.bin" => return label(Category::Cleanable, tr!("Lixeira", "Recycle Bin")),
                "system volume information" => {
                    return label(
                        Category::System,
                        tr!("pontos de restauração", "restore points"),
                    );
                }
                "windows" => return label(Category::System, "Windows"),
                "windows.old" => {
                    return label(
                        Category::Cleanable,
                        tr!(
                            "Windows antigo, de uma atualização",
                            "old Windows, from an upgrade"
                        ),
                    );
                }
                _ => {}
            }
        }
        if n == 2 && parts[0] == "windows" {
            match last {
                "winsxs" => {
                    return label(
                        Category::System,
                        tr!("componentes do Windows", "Windows components"),
                    );
                }
                "installer" => {
                    return label(
                        Category::System,
                        tr!("instaladores de programas", "program installers"),
                    );
                }
                "softwaredistribution" => {
                    return label(
                        Category::Cleanable,
                        tr!("downloads do Windows Update", "Windows Update downloads"),
                    );
                }
                "temp" => return label(Category::Cleanable, tr!("temporários", "temporary files")),
                _ => {}
            }
        }
        if last == "temp" && at(1) == Some("local") && at(2) == Some("appdata") {
            return label(Category::Cleanable, tr!("temporários", "temporary files"));
        }
        if matches!(last, "dxcache" | "glcache" | "nv_cache") || last == "shadercache" {
            return label(Category::Cleanable, tr!("cache de shaders", "shader cache"));
        }
        if last == "crashdumps" {
            return label(
                Category::Cleanable,
                tr!("despejos de falhas", "crash dumps"),
            );
        }
        return None;
    }
    // Files.
    if n == 1 {
        match last {
            "pagefile.sys" | "swapfile.sys" => {
                return label(
                    Category::System,
                    tr!("memória virtual do Windows", "Windows virtual memory"),
                );
            }
            "hiberfil.sys" => return label(Category::System, tr!("hibernação", "hibernation")),
            _ => {}
        }
    }
    let ext = last.rsplit_once('.').map(|(_, e)| e).unwrap_or("");
    let in_path = |s: &str| parts.iter().any(|p| p.contains(s));
    match ext {
        "vhdx" | "vhd" if last == "ext4.vhdx" || in_path("wsl") && !in_path("docker") => label(
            Category::VirtualMachines,
            tr!("disco do WSL (Linux)", "WSL disk (Linux)"),
        ),
        "vhdx" | "vhd" if in_path("docker") => label(
            Category::VirtualMachines,
            tr!("disco do Docker", "Docker disk"),
        ),
        "vhdx" | "vhd" | "vmdk" | "vdi" | "qcow2" | "avhdx" => label(
            Category::VirtualMachines,
            tr!("disco de máquina virtual", "virtual machine disk"),
        ),
        "dmp" => label(Category::Cleanable, tr!("despejo de falha", "crash dump")),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn what(p: &str, dir: bool) -> Option<(Category, String)> {
        label_of(Path::new(p), dir).map(|l| (l.category, l.what))
    }

    #[test]
    fn games_of_each_store() {
        let steam = r"C:\Program Files (x86)\Steam\steamapps\common\Cyberpunk 2077";
        assert_eq!(what(steam, true).unwrap().0, Category::Games);
        assert!(what(steam, true).unwrap().1.contains("Steam"));
        assert!(what(r"D:\SteamLibrary\steamapps\common\ELDEN RING", true).is_some());
        assert!(
            what(r"C:\Riot Games\League of Legends", true)
                .unwrap()
                .1
                .contains("Riot")
        );
        assert!(what(r"C:\Riot Games\Riot Client", true).is_none());
        assert!(
            what(r"C:\XboxGames\Forza Horizon 5", true)
                .unwrap()
                .1
                .contains("Xbox")
        );
        assert!(what(r"C:\Program Files\Epic Games\Fortnite", true).is_some());
        assert!(what(r"C:\Program Files\Epic Games\Launcher", true).is_none());
        assert!(what(r"C:\Program Files\Rockstar Games\Grand Theft Auto V", true).is_some());
        assert!(what(r"C:\Program Files\Rockstar Games\Social Club", true).is_none());
        assert!(
            what(
                r"C:\Program Files (x86)\Steam\steamapps\common\Steamworks Shared",
                true
            )
            .is_none()
        );
        // The game's own subfolders are not games.
        assert!(what(&format!(r"{steam}\bin"), true).is_none());
    }

    #[test]
    fn system_and_virtual_disks() {
        assert_eq!(what(r"C:\pagefile.sys", false).unwrap().0, Category::System);
        assert_eq!(what(r"C:\hiberfil.sys", false).unwrap().0, Category::System);
        assert!(what(r"D:\data\pagefile.sys", false).is_none());
        assert_eq!(
            what(r"C:\Windows\WinSxS", true).unwrap().0,
            Category::System
        );
        assert_eq!(
            what(r"C:\Windows.old", true).unwrap().0,
            Category::Cleanable
        );
        assert_eq!(
            what(r"C:\$Recycle.Bin", true).unwrap().0,
            Category::Cleanable
        );
        let wsl = r"C:\Users\ana\AppData\Local\wsl\{99f4}\ext4.vhdx";
        assert!(what(wsl, false).unwrap().1.contains("WSL"));
        let docker = r"C:\Users\ana\AppData\Local\Docker\wsl\disk\docker_data.vhdx";
        assert!(what(docker, false).unwrap().1.contains("Docker"));
        assert!(what(r"D:\VMs\win.vmdk", false).is_some());
        assert_eq!(
            what(r"C:\Users\ana\AppData\Local\Temp", true).unwrap().0,
            Category::Cleanable
        );
        assert!(what(r"C:\Users\ana\Documents", true).is_none());
        assert!(what(r"C:\Users\ana\notes.txt", false).is_none());
    }
}

/// One recognized item in the map: its node, size and what it is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Thing {
    pub node: u32,
    pub bytes: u64,
    pub label: Label,
}

/// Files smaller than this are not checked (only the big ones matter).
const THING_MIN_FILE: u64 = 64 * 1024 * 1024;
/// How deep below `root` the search goes, and how many nodes it looks at.
const THING_DEPTH: usize = 8;
const THING_BUDGET: usize = 80_000;

/// The recognized items below `root`, largest first (a recognized folder is
/// not searched inside).
pub fn things(tree: &crate::disk_tree::DiskTree, root: u32) -> Vec<Thing> {
    let mut out = Vec::new();
    let mut seen = 0usize;
    let mut stack = vec![(root, tree.path_of(root), 0usize)];
    if let Some(label) = label_of(&stack[0].1, tree.is_dir(root)) {
        return vec![Thing {
            node: root,
            bytes: tree.shown(root),
            label,
        }];
    }
    while let Some((dir, path, depth)) = stack.pop() {
        for c in tree.children(dir) {
            seen += 1;
            if seen > THING_BUDGET {
                break;
            }
            if tree.is_rest(c) {
                continue;
            }
            let is_dir = tree.is_dir(c);
            if !is_dir && tree.bytes(c) < THING_MIN_FILE {
                continue;
            }
            let child = path.join(tree.os_name(c));
            match label_of(&child, is_dir) {
                Some(label) => out.push(Thing {
                    node: c,
                    bytes: tree.shown(c),
                    label,
                }),
                None if is_dir && depth + 1 < THING_DEPTH && tree.shown(c) > 0 => {
                    stack.push((c, child, depth + 1));
                }
                None => {}
            }
        }
    }
    out.retain(|t| t.bytes > 0);
    out.sort_by_key(|t| std::cmp::Reverse(t.bytes));
    out
}

#[cfg(test)]
mod thing_tests {
    use super::*;
    use crate::disk_tree::{DiskTree, Listed, ROOT};
    use std::path::PathBuf;

    #[test]
    fn finds_games_and_disks_in_the_tree() {
        let d = |n: &str| Listed {
            name: n.into(),
            is_dir: true,
            size: 0,
        };
        let f = |n: &str, size: u64| Listed {
            name: n.into(),
            is_dir: false,
            size,
        };
        let gb = 1u64 << 30;
        let mut t = DiskTree::new(PathBuf::from(r"C:\"));
        let top = t.add_listing(ROOT, vec![d("Riot Games"), f("pagefile.sys", 16 * gb)]);
        let games = t.add_listing(top[0].0, vec![d("League of Legends"), d("Riot Client")]);
        t.add_listing(games[0].0, vec![f("game.wad", 30 * gb)]);
        t.add_listing(games[1].0, vec![f("client.exe", gb)]);
        let found = things(&t, ROOT);
        let names: Vec<(&str, Category)> = found
            .iter()
            .map(|x| (t.name(x.node), x.label.category))
            .collect();
        assert_eq!(
            names,
            [
                ("League of Legends", Category::Games),
                ("pagefile.sys", Category::System)
            ]
        );
        assert_eq!(found[0].bytes, 30 * gb);
    }
}
