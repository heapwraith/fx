# fx

Gerenciador de arquivos para Windows 10/11: abre qualquer pasta na hora, acha qualquer arquivo do disco em milissegundos e pode substituir o Explorador. Open source, escrito em Rust.

> Status: **Fase 0 — prova de velocidade.** Ainda não é utilizável.

## Benchmarks

Fase 0, num Ryzen 7 5700X com NVMe e monitor de 239 Hz, cache quente, build release. Detalhes, método e pendências em [`bench/results/gate-0.md`](bench/results/gate-0.md).

| Medida | Meta | fx | Explorador | Files |
|---|---|---|---|---|
| Pasta com 100 mil arquivos, primeira pintura (app já aberto) | ≤ 100 ms | **24 ms** | 264 ms | — |
| Pasta com 100 mil arquivos, janela nova (processo novo) | ≤ 100 ms | 386–406 ms ¹ | 1.048 ms | — |
| Lista completa e ordenada, 100 mil (app aberto) | ≤ 300 ms | **86–117 ms** | — | — |
| Tecla → quadro (digitar para filtrar), p95 | ≤ 16 ms | **3,4 ms** | — | — |
| Busca por nome, por tecla, p95 | ≤ 50 ms | **7,0 ms** ² | — | — |
| Memória parado | ≤ 80 MB | 72 MB | — | — |

¹ Limitado pela inicialização do driver da GPU (~200 ms); ver `docs/DECISIONS.md` D-013. ² Índice sintético de 2 milhões de arquivos; a medida no disco C: real exige Administrador e está pendente. O Files não estava instalado na máquina de teste.

## Desenvolver

Requisitos: Windows 10/11 com WSL, e no WSL: [rustup](https://rustup.rs) (o toolchain é instalado sozinho a partir de `rust-toolchain.toml`), [`cargo-xwin`](https://github.com/rust-cross/cargo-xwin), `clang`, `lld` e `llvm-rc`. O build é cruzado para Windows e o app roda no Windows pela interoperabilidade do WSL (ver `docs/DECISIONS.md`, D-010).

```bash
cargo xwin build
cargo xwin run -p fx-ui --release
```

Documentação para quem contribui (pessoa ou agente): comece por [`CLAUDE.md`](CLAUDE.md) e [`docs/`](docs/).

## Licença

Dupla licença, à sua escolha: [MIT](LICENSE-MIT) ou [Apache 2.0](LICENSE-APACHE). O código copiado em `third_party/gpui` (GPUI, da Zed Industries) é Apache 2.0 e traz um patch próprio, descrito em `third_party/gpui/FX-PATCH.md`.
