# gpui 0.2.2 com patch do fx

Cópia do crate `gpui` 0.2.2 do crates.io (licença Apache-2.0, ver `LICENSE-APACHE`), usada pelo workspace via `[patch.crates-io]`. Motivo: compilar de forma cruzada do WSL para `x86_64-pc-windows-msvc` (docs/DECISIONS.md D-005 e D-007).

A pasta `examples/` do crate foi removida (4,8 MB que não entram no build). O diff completo contra o upstream, sem contar essa remoção (os blocos `[[example]]` do `Cargo.toml` aparecem nele), está em `fx.patch`. Cada trecho alterado tem um comentário `fx patch:`.

| Arquivo | Mudança | Por quê |
|---|---|---|
| `build.rs` | O módulo `windows` é escolhido pelo alvo (`CARGO_CFG_TARGET_OS`), não por `#[cfg(target_os)]`, e não compila mais shaders com `fxc.exe`. | No upstream, o `#[cfg]` testa o *host* do build script, então no WSL o manifesto não era embutido e os shaders não eram gerados. O `fxc.exe` só existe no Windows. |
| `build.rs` | O `.rc` é regravado em `OUT_DIR` com o caminho absoluto do manifesto. | O `llvm-rc` (usado na compilação cruzada) resolve caminhos a partir do `.rc` pré-processado. |
| `build.rs`, `Cargo.toml` | A checagem WGSL só roda se o alvo não for Windows, e o `naga` virou build-dependency incondicional. | O `#[cfg]` do build script é do host (Linux), mas a dependência era escolhida pelo alvo. |
| `Cargo.toml` | `embed-resource` virou build-dependency incondicional. | Mesmo motivo: estava condicionado ao alvo, mas é usado pelo build script no host. |
| `src/platform/windows/directx_renderer.rs`, `compile-shaders.ps1`, `shaders-dxbc/` | Os shaders HLSL são pré-compilados para DXBC por `compile-shaders.ps1`, que usa o `d3dcompiler_47.dll` do próprio Windows, e embutidos com `include_bytes!`. O bytecode fica versionado em `shaders-dxbc/`. | Substitui o `fxc.exe` (só existe no Windows SDK) e a leitura dos `.hlsl` de `CARGO_MANIFEST_DIR`. A primeira versão do patch compilava em runtime com `D3DCompile`, mas isso custava ~250 ms na abertura (medido na T0.5). |
| `src/platform/windows/directx_devices.rs` | O dispositivo D3D11 criado para testar o adaptador é reaproveitado. | O upstream criava dois dispositivos, e cada criação custa ~200 ms no driver da NVIDIA na partida do processo (medido na T0.5). |

**Se um `.hlsl` mudar** (só acontece ao atualizar o gpui), rode `powershell.exe -File third_party/gpui/compile-shaders.ps1` no Windows e faça commit de `shaders-dxbc/`.

**Para atualizar o gpui:** copie a nova versão do registry, reaplique `fx.patch` (`patch -p1 < fx.patch`, resolvendo conflitos) e regenere o diff.
