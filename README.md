# fx

Gerenciador de arquivos para Windows 10/11: abre qualquer pasta na hora, acha qualquer arquivo do disco em milissegundos e pode substituir o Explorador. Open source, escrito em Rust.

> Status: **Fase 0 — prova de velocidade.** Ainda não é utilizável.

## Benchmarks

Preenchido ao fim da Fase 0 (`bench/results/gate-0.md`).

| Medida | Meta | fx | Explorador | Files |
|---|---|---|---|---|
| Pasta com 100 mil arquivos, primeira pintura | ≤ 100 ms | — | — | — |
| Busca por nome no disco inteiro, por tecla | ≤ 50 ms | — | — | — |

## Desenvolver

Requisitos: Windows 10/11 com WSL, e no WSL: [rustup](https://rustup.rs) (o toolchain é instalado sozinho a partir de `rust-toolchain.toml`), [`cargo-xwin`](https://github.com/rust-cross/cargo-xwin), `clang`, `lld` e `llvm-rc`. O build é cruzado para Windows e o app roda no Windows pela interoperabilidade do WSL (ver `docs/DECISIONS.md`, D-010).

```bash
cargo xwin build
cargo xwin run -p fx-ui --release
```

Documentação para quem contribui (pessoa ou agente): comece por [`CLAUDE.md`](CLAUDE.md) e [`docs/`](docs/).

## Licença

Dupla licença, à sua escolha: [MIT](LICENSE-MIT) ou [Apache 2.0](LICENSE-APACHE). O código copiado em `third_party/gpui` (GPUI, da Zed Industries) é Apache 2.0 e traz um patch próprio, descrito em `third_party/gpui/FX-PATCH.md`.
