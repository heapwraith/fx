# CLAUDE.md — fx (gerenciador de arquivos para Windows)

> Nome provisório: **fx**. Os crates usam o prefixo `fx-`. Trocar o nome depois é só buscar e substituir.

## O que é este projeto

Um gerenciador de arquivos para Windows 10/11, open source (MIT OR Apache-2.0), que:
1. abre qualquer pasta na hora (100 mil arquivos em ≤ 100 ms),
2. acha qualquer arquivo do disco em milissegundos (lendo a MFT do NTFS),
3. pode substituir o Explorador como padrão do sistema.

Leia antes de qualquer tarefa:
- `docs/ARCHITECTURE.md` — crates, threads, processos, o que pode depender do quê.
- `docs/PERFORMANCE.md` — metas numéricas e o protocolo de medição.
- `docs/SAFETY.md` — regras de segurança de dados e limites de autonomia. **Obrigatório.**
- A fase atual em `docs/phases/` (hoje: `PHASE-0.md`).
- `docs/DECISIONS.md` — decisões já tomadas. Não reabra sem motivo novo e registrado.

## Ambiente (crítico)

Este é um app **nativo do Windows**, desenvolvido **no WSL** e compilado de forma cruzada (D-010). Janelas, MFT e shell só funcionam quando o `.exe` roda no Windows, o que a interoperabilidade do WSL faz automaticamente.

- O código e o Claude Code ficam no WSL. O build usa **`cargo xwin`** (não `cargo`), que fornece o CRT e o SDK do MSVC e o linker `lld-link`. Requer `cargo-xwin`, `clang`, `lld` e `llvm-rc` no WSL.
- `.cargo/config.toml` fixa o alvo `x86_64-pc-windows-msvc`. Toolchain fixado em `rust-toolchain.toml` (Rust 1.100.0).
- `cargo xwin run` e `cargo xwin test` executam os `.exe` no Windows. Pastas de teste e benchmark ficam no Windows (`%TEMP%`), nunca no sistema de arquivos do WSL.
- O GPUI vem de `third_party/gpui` com patch (D-007, `third_party/gpui/FX-PATCH.md`). Não edite esse crate sem atualizar `fx.patch`.
- Testes que leem a MFT precisam de **processo Windows elevado (Administrador)**. Eles ficam marcados com `#[ignore]` e rodam só quando o usuário pedir. A forma de elevar a partir do WSL será definida quando chegarmos neles.

## Comandos

```bash
cargo xwin build                                                   # compila o workspace
cargo xwin build --release                                         # build otimizado (use para medir desempenho)
cargo xwin test --workspace                                        # testes que não precisam de admin (rodam no Windows)
cargo xwin test --workspace -- --ignored                           # testes que precisam de admin (só elevado)
cargo xwin clippy --target x86_64-pc-windows-msvc --workspace --all-targets -- -D warnings
cargo fmt --all
cargo xwin run -p fx-ui --release                                  # abre o app no Windows
cargo xwin run -p fx-bench --release -- fixtures                   # gera as pastas de teste em %TEMP%\fx-fixtures
cargo xwin run -p fx-bench --release -- all                        # roda todos os benchmarks e grava bench/results/*.json
```

O `clippy` precisa de `--target` explícito: sem ele, o `cargo xwin` repete o `--target` nos argumentos do rustc e falha.

Antes de dizer que uma tarefa terminou: `cargo fmt --all`, o `clippy` acima com `-D warnings` e `cargo xwin test --workspace` precisam passar. Se a tarefa mexe em desempenho, rode o benchmark correspondente em `--release` e cole os números no relatório.

## Regras de código

- **Thread de UI nunca bloqueia.** Nada de I/O de disco, rede, COM do shell ou espera de lock na thread de UI. Trabalho pesado vai para os workers (ver ARCHITECTURE.md). Se algo pode demorar mais que 1 ms, não roda na UI.
- **`unsafe` só em `fx-shell`, `fx-fs` e `fx-index`**, dentro de funções pequenas, cada bloco com um comentário `// SAFETY:` explicando por que é seguro. `fx-core` e `fx-ui` têm `#![forbid(unsafe_code)]`.
- **Erros:** `thiserror` nos crates de biblioteca, `anyhow` só nos binários. Sem `unwrap()`/`expect()` fora de testes, exceto invariantes óbvias com comentário.
- **Strings de caminho:** use `PathBuf`/`OsString`. Nunca converta caminho para `String` com perda (`to_string_lossy`) exceto para exibição. Suporte caminhos longos (`\\?\`) e nomes Unicode.
- **Dependências:** antes de adicionar um crate, verifique se já existe algo no workspace. Toda dependência nova vai em `[workspace.dependencies]` com versão exata e é registrada em `docs/DECISIONS.md` com o motivo.
- **APIs que você não tem certeza:** a API do GPUI muda entre versões e a do windows-rs depende de *features*. Não confie na memória: leia o código-fonte da versão fixada em `~/.cargo/registry/src` ou os exemplos do próprio crate antes de usar.
- **Testes:** toda função de lógica (ordenação, filtro, busca, fila de operações, desfazer) tem teste unitário em `fx-core` ou no crate dela. Testes que tocam o disco usam pastas temporárias (`tempfile`), nunca pastas reais do usuário.
- Identificadores e comentários de código em inglês; documentação em `docs/` em português.

## Como trabalhar

1. Pegue a próxima tarefa não concluída da fase atual (`docs/phases/PHASE-N.md`), na ordem.
2. Leia os critérios de aceite dela. Se algo estiver ambíguo, pergunte antes de começar.
3. Faça a menor implementação que cumpre os critérios. Não adiante tarefas de fases futuras.
4. Verifique (fmt, clippy, testes, benchmark se aplicável).
5. Marque a tarefa como concluída no arquivo da fase, com a data e um resumo de uma linha (e os números, se houver).
6. Faça um commit por tarefa: `T0.3: lista virtualizada com 100k itens (82 ms)`.
7. Ao fim de cada fase, rode `/gate` e pare: a decisão de passar o portão é do usuário.

## Comandos personalizados

- `/next` — pega a próxima tarefa da fase atual e começa.
- `/bench` — roda os benchmarks em release e compara com as metas.
- `/gate` — verifica os critérios do portão da fase atual e escreve o relatório em `bench/results/gate-N.md`.
