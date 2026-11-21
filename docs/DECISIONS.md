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
- **Data:** 2026-11-20 · **Status:** aceita (confirmada pelo usuário em 2026-11-21, ao publicar em github.com/heapwraith/fx)
- **Decisão:** dupla licença MIT OR Apache-2.0 (`LICENSE-MIT`, `LICENSE-APACHE`), o padrão do ecossistema Rust e compatível com o GPUI (Apache-2.0). A GPL-3.0 foi descartada. O código copiado em `third_party/gpui` continua sob a licença dele (Apache-2.0, `third_party/gpui/LICENSE-APACHE`).

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
- **Consequência:** manter o patch a cada atualização do gpui. **Atualizado em 2026-11-21 (T0.5):** compilar os shaders em runtime custava ~250 ms na abertura, então agora eles são pré-compilados com o `d3dcompiler_47.dll` do Windows (`compile-shaders.ps1`) e embutidos no binário. O patch também passou a reaproveitar o dispositivo D3D11 do teste de adaptador (−200 ms). Detalhes em `third_party/gpui/FX-PATCH.md`.

## D-008 — Acessibilidade (UI Automation) com GPUI
- **Data:** 2026-11-21 · **Status:** aceita (resultado: **não funciona**; caminho proposto abaixo)
- **Verificação (T0.9):**
  - **Código do gpui 0.2.2:** nada de UI Automation no Windows. Não responde ao `WM_GETOBJECT`, não implementa provedores UIA (`IRawElementProvider*`) e não usa o AccessKit.
  - **Na prática:** abri o fx na `flat-100k` e percorri a árvore de UI Automation da janela (`System.Windows.Automation`, a mesma árvore que o Narrador lê). Ela tem **um único elemento**, a janela (`Zed::Window`), **sem filhos**: o Narrador não enxerga a lista, os itens, o item selecionado nem o cabeçalho. Antes desta tarefa a janela nem tinha nome; agora tem título (`<pasta> — fx`), então o Narrador ao menos anuncia a janela.
  - **Narrador ao vivo:** a verificação foi feita pela árvore UIA, não ouvindo o Narrador. Para confirmar, abra o fx e pressione Ctrl+Win+Enter.
- **Caminho proposto:** camada própria de UI Automation com o **AccessKit** (`accesskit` + `accesskit_windows` 0.35.2, MIT OR Apache-2.0, usa `windows` 0.62 como o nosso workspace). O `accesskit_windows::SubclassingAdapter` trata o `WM_GETOBJECT` subclassificando um HWND que já existe, sem precisar de patch no gpui. A única exigência é ser instalado **antes** de a janela aparecer, e o gpui permite isso: abrir com `WindowOptions { show: false }`, pegar o HWND por `HasWindowHandle`, instalar o adaptador e só então chamar `window.activate_window()`. A árvore (janela → cabeçalho → lista com N itens → só as linhas visíveis, como na virtualização) é montada a partir do estado da `FolderView`, com foco e seleção atualizados por `update_if_active`.
  - Alternativas: contribuir com o upstream do GPUI (a Zed não tem isso pronto no Windows, prazo incerto) ou o plano B (D-002), que teria que implementar UIA de qualquer jeito.
- **Consequência:** a acessibilidade tem caminho viável sem trocar de stack (critério 3 do portão). Implementação na Fase 1, com duas dependências novas (`accesskit`, `accesskit_windows`), a confirmar quando a tarefa for detalhada. Risco: o AccessKit ainda não cobre listas virtualizadas enormes de forma nativa; precisaremos expor só a janela visível e o total de itens.

## D-009 — Estratégia de enumeração de pastas
- **Data:** 2026-11-21 · **Status:** aceita
- **Contexto (T0.3):** três estratégias atrás da mesma interface (`fx_fs::enumerate`, lotes de 2.000 entradas ou 8 ms por canal). Medidas na `flat-100k` (100 mil arquivos), em release, cache quente (1 aquecimento descartado), 10 execuções por estratégia, 3 rodadas completas. Máquina: AMD Family 25 Model 33 (Zen 3), 16 núcleos lógicos, Windows 10.0.26200, SSD do `%TEMP%`. Medido do lado de quem recebe os lotes (a visão da UI).

  | Estratégia | 1º lote, mediana / p95 | Total, mediana / p95 |
  |---|---|---|
  | `std::fs::read_dir` | 1,9–2,0 / 2,0–2,4 ms | 51–54 / 54–59 ms |
  | `FindFirstFileExW` (Basic + LARGE_FETCH) | 2,8 / 3,1–3,7 ms | **49–51 / 53–58 ms** |
  | `GetFileInformationByHandleEx(FileIdBothDirectoryInfo)`, buffer de 256 KB | 3,0–3,3 / 3,4–3,8 ms | 89–90 / 95–96 ms |

  Metas da T0.3: 1º lote ≤ 20 ms e total ≤ 150 ms. As três passam. O `FileIdBothDirectoryInfo` é ~75% mais lento. O parser não é a causa: tirar a cópia dos nomes e usar buffer de 64 KB não melhorou (92 ms e 98 ms). O custo está na própria classe de informação (FileId e nome curto 8.3 por entrada).
- **Decisão:** `Strategy::FindFirstFileEx` é a padrão: menor tempo total em todas as rodadas, e o total é o que pesa na P3. A diferença para `std::fs::read_dir` é pequena (~5%); o `std` tem o primeiro lote ~1 ms mais rápido, irrelevante perto da meta de 20 ms. As três implementações ficam no código para remedição (`fx-bench enum`).
- **Consequência:** a enumeração consome ~50 ms dos 300 ms da P3 (lista completa e ordenada). Se precisar de mais, o próximo passo é medir `FileFullDirectoryInfo` (sem FileId e sem nome curto) ou `NtQueryDirectoryFile`, e paralelizar a montagem das entradas.

## D-010 — Ambiente de desenvolvimento: WSL com compilação cruzada para Windows
- **Data:** 2026-11-21 · **Status:** aceita (substitui D-005)
- **Contexto:** o usuário decidiu não desenvolver no Windows nativo.
- **Decisão:** o código e o Claude Code ficam no WSL. O build é cruzado para `x86_64-pc-windows-msvc` com `cargo xwin` (CRT e SDK da Microsoft baixados pelo xwin, linker `lld-link`, `clang-cl` e `llvm-rc`). `.cargo/config.toml` fixa esse alvo como padrão. Os `.exe` (app, testes e benchmarks) rodam no Windows pela interoperabilidade do WSL: `cargo xwin run` e `cargo xwin test` os executam direto.
- **Consequência:** todo comando de build usa `cargo xwin` em vez de `cargo`. Dependências cujo `build.rs` assume host Windows precisam de patch (ex.: gpui, D-007). Testes e benchmarks que exigem Administrador precisam que o processo Windows rode elevado: a definir quando chegarmos neles (T0.5+). Os benchmarks de disco devem usar pastas do Windows (`%TEMP%`), não do sistema de arquivos do WSL.

## D-011 — Dependência: `tempfile` (dev) e fixtures sem dependências
- **Data:** 2026-11-21 · **Status:** aceita
- **Decisão:** `tempfile = "=3.27.0"` em `[workspace.dependencies]`, usado só como dev-dependency (testes que tocam o disco, conforme o `CLAUDE.md`; previsto na T0.3). O gerador de fixtures (T0.2) não usa crates externos: PRNG SplitMix64 próprio (a sequência não pode mudar com a versão de um crate) e codificador PNG mínimo sem compressão (são só dados de teste).
- **Consequência:** as imagens de `flat-10k-media` têm cerca de 12 KB cada (~124 MB no total), porque não são comprimidas. `clippy.toml` permite `unwrap()`/`expect()` em testes, como já dizia o `CLAUDE.md`.

## D-012 — Plataformas: Windows primeiro, portável depois
- **Data:** 2026-11-21 · **Status:** aceita
- **Contexto:** o projeto nasceu só para Windows sem registro do motivo. Dois dos três objetivos são específicos do Windows: a busca lendo a MFT do NTFS e substituir o Explorador. Já o GPUI e o `fx-core` são portáveis.
- **Decisão:** só o Windows é suportado, medido e empacotado até o fim da Fase 4. O código específico do Windows fica isolado atrás de tipos e traits neutros em `fx-fs`, `fx-shell`, `fx-index` e `fx-ops`, com a implementação em `src/windows/` sob `#[cfg(windows)]`, conforme as regras em `docs/ARCHITECTURE.md`, seção "Plataformas". Um port para Linux ou macOS, se for feito, vira uma fase própria com metas próprias. Fora do Windows, a busca exige outra técnica (índice próprio com inotify/fanotify ou FSEvents).
- **Consequência:** custo pequeno agora (organizar os módulos e não vazar tipos do `windows-rs` na API pública) e nenhum port parcial ou stub para manter. Os objetivos e as metas de desempenho continuam valendo só para Windows.

## D-013 — Partida do processo limitada pelo driver da GPU (P1/P2)
- **Data:** 2026-11-21 · **Status:** aceita (decidida no portão da Fase 0; o usuário delegou a escolha)
- **Contexto (T0.5):** primeira pintura da `flat-100k`, mediana de 10 aberturas, release. **Desde o início do processo: 406 ms** (meta P2: 100 ms). **Desde o pedido da pasta, com o app já aberto: 25 ms.** Antes das correções no patch do gpui eram ~800 ms. Do tempo que sobra, ~200 ms são o `D3D11CreateDevice` (o driver da NVIDIA carrega a cada processo novo), ~40 ms o DirectWrite, ~30 ms o carregamento do executável e ~35 ms a criação da janela e da swap chain. Nosso código (ler a pasta, montar a lista e pintar) leva ~25 ms.
- **Opções a decidir:** (a) processo residente, como o Explorador: o fx fica aberto (bandeja ou serviço) e "abrir pasta" cria só uma janela nova; (b) mostrar uma primeira janela sem GPU e passar para o GPUI quando o dispositivo estiver pronto; (c) plano B de stack (D-002), lembrando que qualquer interface acelerada por GPU paga o custo do driver na partida; (d) aceitar a meta para "app já aberto" e redefinir a P1.
- **Decisão:** **(a) processo residente.** O fx fica carregado depois da primeira abertura e "abrir pasta" só cria uma janela no processo existente (instância única por named pipe, na T1.2 da Fase 1). A P2 e a P3 passam a valer para o fx já carregado (medido: 24 ms e 86–117 ms). A P1 vira "primeira abertura do dia", com meta de **≤ 500 ms** (medido: 386–406 ms), porque ~200 ms de inicialização do driver da GPU não dependem do nosso código. A opção (b), janela sem GPU, fica guardada caso a P1 piore.
- **Consequência:** `fx-bench open` continua reportando as duas formas. A Fase 1 inclui a instância única. Fechar a última janela não encerra o processo (configurável na Fase 1, com opção de sair).

## D-014 — Régua da P7: quadros perdidos, não p95 do intervalo
- **Data:** 2026-11-21 · **Status:** aceita (decidida no portão da Fase 0; o usuário delegou a escolha)
- **Contexto (T0.5):** o GPUI desenha no ritmo do monitor (`DwmFlush`). A 239 Hz cada quadro dura 4,18 ms, e o intervalo medido oscila entre ~4,0 e ~4,4 ms sem nenhum quadro perdido. Um quadro perdido aparece como ~8,4 ms. Uma meta "p95 ≤ um período" falha por oscilação do relógio, não por travamento.
- **Decisão:** P7 = **no máximo 0,5% de quadros perdidos** durante a rolagem (intervalo ≥ 1,5× o período do monitor), medido na taxa real do monitor de teste e anotada no relatório. O p95 e o máximo continuam reportados.
- **Consequência:** resultado da Fase 0: 1 quadro perdido em 3.425 (0,03%) passa. `fx-bench scroll` passa a usar esse critério.

## D-015 — Fundação visual (T1.1)
- **Data:** 2026-11-21 · **Status:** aceita
- **Decisão:** tokens Fluent 2 próprios em `fx-ui/src/theme.rs` (sem biblioteca de componentes). Tema claro/escuro seguindo o Windows ao vivo, Segoe UI Variable com Segoe UI como reserva, espaçamentos em múltiplos de 4 px, densidades de 24/32/40 px (padrão 32, Ctrl+1/2/3). O contraste ≥ 4,5:1 é garantido por teste unitário, com as cores semitransparentes compostas sobre a base da Mica.
- **Mica:** variante `MicaBackdrop` no patch do gpui (`third_party/gpui/FX-PATCH.md`). Só é pedida no Windows 11 22H2+ (build ≥ 22621, lido com `RtlGetVersion` no `fx-shell`) e só quando o tema do fx é igual ao do sistema, porque a Mica herda a cor do tema do sistema. Nos outros casos, fundo sólido.
- **Features novas do `windows`** (sem crate novo): `Win32_System_SystemInformation` e `Wdk_System_SystemServices`, para o `RtlGetVersion`. O `GetVersionExW` mente a versão em executáveis sem manifesto, como os de teste.

## D-016 — Instância única e modelo de seleção (T1.2)
- **Data:** 2026-11-21 · **Status:** aceita
- **Instância única (D-013):** o primeiro processo do fx na sessão cria o pipe `\\.\pipe\fx-instance-<sessão>` (`FILE_FLAG_FIRST_PIPE_INSTANCE`, uma instância, modo mensagem, só clientes locais, DACL padrão: só o próprio usuário, SYSTEM e administradores escrevem). Os processos seguintes mandam o caminho absoluto em UTF-16, liberam o primeiro plano (`AllowSetForegroundWindow`) e saem em ~40 ms; o primeiro abre uma janela nova. Se o pipe falhar, o processo roda sozinho. Código em `fx-shell/src/windows/instance.rs`, com API neutra (`claim_instance`, `Instance`).
- **Exceções:** `FX_TRACE=1` (benchmarks) e `FX_SINGLE_INSTANCE=0` não usam a instância única, para que cada processo meça a si mesmo.
- **Ainda não feito:** fechar a última janela ainda encerra o processo (comportamento do GPUI). Ficar residente sem janela exige um jeito visível de sair (ícone na bandeja ou opção nas configurações), que entra com a persistência de configurações (T1.17) ou numa tarefa própria.
- **Seleção:** `fx_core::Selection` guarda entradas (índices da lista), não posições na tela, num bitset com cursor e âncora. Por isso sobrevive a reordenação e filtro. Entradas escondidas pelo filtro continuam selecionadas; operações futuras devem usar `selected_in(rows)`, que devolve só as visíveis.
- **Features novas do `windows`** (sem crate novo): `Win32_System_Pipes` e `Win32_System_RemoteDesktop` (`ProcessIdToSessionId`, para o nome do pipe por sessão).
