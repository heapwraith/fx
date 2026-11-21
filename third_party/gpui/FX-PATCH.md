# gpui 0.2.2 com patch do fx

Cópia do crate `gpui` 0.2.2 do crates.io (licença Apache-2.0, ver `LICENSE-APACHE`), usada pelo workspace via `[patch.crates-io]`. Motivo: compilar de forma cruzada do WSL para `x86_64-pc-windows-msvc` (docs/DECISIONS.md D-005 e D-007).

O diff completo contra o upstream está em `fx.patch`. Cada trecho alterado tem um comentário `fx patch:`.

| Arquivo | Mudança | Por quê |
|---|---|---|
| `build.rs` | O módulo `windows` é escolhido pelo alvo (`CARGO_CFG_TARGET_OS`), não por `#[cfg(target_os)]`, e não compila mais shaders com `fxc.exe`. | No upstream, o `#[cfg]` testa o *host* do build script, então no WSL o manifesto não era embutido e os shaders não eram gerados. O `fxc.exe` só existe no Windows. |
| `build.rs` | O `.rc` é regravado em `OUT_DIR` com o caminho absoluto do manifesto. | O `llvm-rc` (usado na compilação cruzada) resolve caminhos a partir do `.rc` pré-processado. |
| `build.rs`, `Cargo.toml` | A checagem WGSL só roda se o alvo não for Windows, e o `naga` virou build-dependency incondicional. | O `#[cfg]` do build script é do host (Linux), mas a dependência era escolhida pelo alvo. |
| `Cargo.toml` | `embed-resource` virou build-dependency incondicional. | Mesmo motivo: estava condicionado ao alvo, mas é usado pelo build script no host. |
| `src/platform/windows/directx_renderer.rs` | Os shaders HLSL ficam embutidos com `include_str!` e são compilados em runtime com `D3DCompile`, em debug e em release (release com `D3DCOMPILE_OPTIMIZATION_LEVEL3`). | Substitui o bytecode pré-compilado pelo `fxc.exe` (release) e a leitura dos `.hlsl` de `CARGO_MANIFEST_DIR` (debug), que aponta para um caminho Linux. O `d3dcompiler_47.dll` vem com o Windows 10/11. |

**Custo conhecido:** a compilação dos shaders acontece na abertura do app, uma vez por processo. Ela entra na medição de inicialização da Fase 0. Se pesar, a alternativa é pré-compilar com o `fxc.exe` via interop (exige o Windows SDK no Windows).

**Para atualizar o gpui:** copie a nova versão do registry, reaplique `fx.patch` (`patch -p1 < fx.patch`, resolvendo conflitos) e regenere o diff.
