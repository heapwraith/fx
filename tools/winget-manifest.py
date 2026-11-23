#!/usr/bin/env python3
"""Writes the winget manifests of a release (T5.2), ready to send to
microsoft/winget-pkgs once the release is published on GitHub (the URLs must
answer and the hashes match the files there).

    python3 tools/winget-manifest.py 1.0.0 dist/Pathdash-1.0.0-setup-x64.exe dist/Pathdash-1.0.0-setup-arm64.exe

Writes packaging/winget/manifests/h/heapwraith/Pathdash/<version>/ with the three
files of manifest schema 1.9 (version, installer, defaultLocale). The
installer is Pathdash's own (`fx-setup`): per-user, no administrator, `--silent`.
"""

import hashlib
import pathlib
import sys

ID = "heapwraith.Pathdash"
REPO = "https://github.com/heapwraith/fx"
SCHEMA = "1.9.0"


def sha256(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest().upper()


def main():
    if len(sys.argv) < 3:
        sys.exit(__doc__)
    version, installers = sys.argv[1], sys.argv[2:]
    out = pathlib.Path("packaging/winget/manifests/h/heapwraith/Pathdash") / version
    out.mkdir(parents=True, exist_ok=True)
    head = f"# yaml-language-server: $schema=https://aka.ms/winget-manifest.{{}}.{SCHEMA}.schema.json\n"
    (out / f"{ID}.yaml").write_text(
        head.format("version")
        + f"PackageIdentifier: {ID}\nPackageVersion: {version}\nDefaultLocale: pt-BR\n"
        + f"ManifestType: version\nManifestVersion: {SCHEMA}\n",
        encoding="utf-8",
        newline="\n",
    )
    entries = []
    for path in installers:
        name = pathlib.Path(path).name
        arch = "arm64" if "arm64" in name else "x64"
        entries.append(
            f"  - Architecture: {arch}\n"
            f"    InstallerUrl: {REPO}/releases/download/v{version}/{name}\n"
            f"    InstallerSha256: {sha256(path)}\n"
        )
    (out / f"{ID}.installer.yaml").write_text(
        head.format("installer")
        + f"PackageIdentifier: {ID}\nPackageVersion: {version}\n"
        + "InstallerType: exe\nScope: user\nInstallModes:\n  - silent\n"
        + "InstallerSwitches:\n  Silent: --silent\n  SilentWithProgress: --silent\n"
        + "UpgradeBehavior: install\nMinimumOSVersion: 10.0.19041.0\n"
        + "Installers:\n"
        + "".join(entries)
        + f"ManifestType: installer\nManifestVersion: {SCHEMA}\n",
        encoding="utf-8",
        newline="\n",
    )
    (out / f"{ID}.locale.pt-BR.yaml").write_text(
        head.format("defaultLocale")
        + f"PackageIdentifier: {ID}\nPackageVersion: {version}\nPackageLocale: pt-BR\n"
        + "Publisher: Pathdash\nPublisherUrl: " + REPO + "\nPackageName: Pathdash\n"
        + "PackageUrl: " + REPO + "\nLicense: MIT OR Apache-2.0\n"
        + f"LicenseUrl: {REPO}/blob/main/LICENSE-MIT\n"
        + "ShortDescription: Gerenciador de arquivos rápido para Windows, com busca no disco inteiro enquanto você digita.\n"
        + "Description: |-\n"
        + "  Abre qualquer pasta na hora, acha qualquer arquivo do disco em milissegundos lendo a MFT do NTFS,\n"
        + "  tem abas, painéis, Quick Look, terminal embutido, etiquetas e desfazer de qualquer operação,\n"
        + "  e pode substituir o Explorador como gerenciador padrão. Sem telemetria.\n"
        + "Tags:\n  - file-manager\n  - explorer\n  - search\n  - gerenciador-de-arquivos\n  - pathdash\n"
        + f"ReleaseNotesUrl: {REPO}/blob/main/CHANGELOG.md\n"
        + f"ManifestType: defaultLocale\nManifestVersion: {SCHEMA}\n",
        encoding="utf-8",
        newline="\n",
    )
    print(f"written: {out}")


if __name__ == "__main__":
    main()
