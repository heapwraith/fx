# Registro de decisões

Formato: cada decisão tem ID, data, status (aceita / substituída / pendente), contexto, decisão e consequência. Não apague decisões antigas: marque como "substituída por D-XXX".

---

## D-001 — Linguagem: Rust
- **Data:** 2026-11-20 · **Status:** aceita
- **Contexto:** app que roda o dia todo, precisa de desempenho de C++ e acesso total à API do Windows.
- **Decisão:** Rust estável, fixado em `rust-toolchain.toml` (1.100.0, lançado em 12/11/2026). Acesso ao Windows pelo crate oficial `windows` (windows-rs) 0.62.x.
- **Consequência:** segurança de memória; `unsafe` restrito a `fx-fs`, `fx-index`, `fx-shell`.

## D-002 — Interface: GPUI (com plano B)
- **Data:** 2026-11-20 · **Status:** aceita, sujeita ao portão da Fase 0
- **Contexto:** o Files (WinUI 3/WinAppSDK) é lento, e os próprios mantenedores admitem que o framework fica aquém em desempenho. O File Pilot mostra que renderização direta na GPU resolve.
- **Decisão:** GPUI (framework do editor Zed, DirectX 11 + DirectWrite no Windows, Apache-2.0). Plano B: renderizador próprio em Direct3D 11 + DirectWrite.
- **Rejeitadas:** WinUI 3 (desempenho), Tauri/Electron (peso), egui/iced (texto e visual menos nativos).
- **Consequência:** API pré-1.0 → versão fixada (D-007) e atualizações só deliberadas.

## D-003 — Busca: leitura direta da MFT em serviço separado
- **Data:** 2026-11-20 · **Status:** aceita
- **Decisão:** índice a partir da MFT + diário USN (crate `ntfs-reader` 0.4.6 como ponto de partida). Ler a MFT exige admin, então em produção ela fica no serviço `fx-indexd`; o app roda sem elevação.
- **Consequência:** só NTFS tem busca instantânea; FAT/exFAT/rede caem para enumeração normal.

## D-004 — Licença: MIT OR Apache-2.0
- **Data:** 2026-11-20 · **Status:** pendente de confirmação do usuário (alternativa em discussão: GPL-3.0)

## D-005 — Ambiente de desenvolvimento: Windows nativo
- **Data:** 2026-11-20 · **Status:** substituída por D-010
- **Decisão:** desenvolvimento e Claude Code rodam nativos no Windows, não no WSL, porque o alvo é `x86_64-pc-windows-msvc` e as APIs de COM/NTFS só existem no Windows.

## D-006 — Distribuição e assinatura
- **Data:** 2026-11-20 · **Status:** aceita
- **Decisão:** Microsoft Store (MSIX, assinatura gratuita pela Store) + GitHub Releases assinados via SignPath Foundation (gratuito para open source que se qualifica) + winget. Azure Artifact Signing descartado: para pessoa física, só atende EUA e Canadá. Plano B: certificado OV (US$ 150–300/ano).

## D-007 — Versão exata do GPUI
- **Data:** 2026-11-21 · **Status:** aceita
- **Contexto:** o crate oficial `gpui` 0.2.2 (crates.io) não tem `gpui_platform` separado nessa versão; a plataforma Windows vem embutida. Ele abre janela e desenha texto com DirectWrite no Windows, mas o `build.rs` só gera shaders (com `fxc.exe`) e embute o manifesto quando o *host* do build é Windows, o que quebra a compilação cruzada do WSL (D-010).
- **Decisão:** `gpui = "=0.2.2"` em `[workspace.dependencies]`, resolvido via `[patch.crates-io]` para uma cópia com patch em `third_party/gpui`. O patch compila os shaders HLSL em runtime com `D3DCompile` (o `d3dcompiler_47.dll` vem no Windows) e faz o `build.rs` decidir pelo alvo. Detalhes e diff em `third_party/gpui/FX-PATCH.md` e `third_party/gpui/fx.patch`. Forks (`gpui-ce`, `gpui-box`) não foram necessários.
- **Consequência:** manter o patch a cada atualização do gpui (são 3 arquivos). A compilação dos shaders custa alguns milissegundos na abertura do app e entra na medição de inicialização. Se pesar, pré-compilar com o `fxc.exe` via interop.

## D-008 — Acessibilidade (UI Automation) com GPUI
- **Status:** pendente (definida na T0.9)

## D-009 — Estratégia de enumeração de pastas
- **Status:** pendente (definida na T0.3, com números)

## D-010 — Ambiente de desenvolvimento: WSL com compilação cruzada para Windows
- **Data:** 2026-11-21 · **Status:** aceita (substitui D-005)
- **Contexto:** o usuário decidiu não desenvolver no Windows nativo.
- **Decisão:** o código e o Claude Code ficam no WSL. O build é cruzado para `x86_64-pc-windows-msvc` com `cargo xwin` (CRT e SDK da Microsoft baixados pelo xwin, linker `lld-link`, `clang-cl` e `llvm-rc`). `.cargo/config.toml` fixa esse alvo como padrão. Os `.exe` (app, testes e benchmarks) rodam no Windows pela interoperabilidade do WSL: `cargo xwin run` e `cargo xwin test` os executam direto.
- **Consequência:** todo comando de build usa `cargo xwin` em vez de `cargo`. Dependências cujo `build.rs` assume host Windows precisam de patch (ex.: gpui, D-007). Testes e benchmarks que exigem Administrador precisam que o processo Windows rode elevado: a definir quando chegarmos neles (T0.5+). Os benchmarks de disco devem usar pastas do Windows (`%TEMP%`), não do sistema de arquivos do WSL.
