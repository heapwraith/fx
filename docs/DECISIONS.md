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

## D-017 — Lateral: unidades por consulta periódica (T1.5)
- **Data:** 2026-11-21 · **Status:** aceita
- **Decisão:** o `fx-shell` descobre unidades novas consultando `GetLogicalDrives` a cada 250 ms numa thread própria (`watch_drives`), em vez de `WM_DEVICECHANGE`. A chamada custa microssegundos; o procedimento de janela pertence ao GPUI e o fx não teria onde receber a mensagem sem outro patch. Quando a máscara de letras muda, a thread relê rótulo, tipo e espaço de cada unidade (chamadas que podem demorar em unidade de rede) e manda a lista; o espaço livre é relido a cada 30 s.
- **Medido:** unidade criada ou removida com `subst` aparece na lateral em 23–60 ms (`fx-bench drives`; meta ≤ 1 s). Um pendrive real cria a letra da mesma forma, mas não foi testado com hardware.
- **Acesso rápido:** começa com Área de Trabalho, Downloads, Documentos, Imagens, Músicas e Vídeos (como o Explorador). Fixar e desafixar fica em memória até a T1.17.

## D-018 — Ícones de arquivo (T1.6)
- **Data:** 2026-11-21 · **Status:** aceita
- **Extração:** `SHGetFileInfoW` (ícone pequeno, 16 px a 100%) numa thread STA do `fx-shell`. Pastas e extensões usam `SHGFI_USEFILEATTRIBUTES` (sem tocar no disco); `.exe`, `.lnk`, `.ico`, `.url`, `.scr`, `.cpl`, `.msc` e `.appref-ms` são extraídos do próprio arquivo. O `HICON` vira RGBA pelo GDI (com a máscara como alfa nos ícones antigos sem canal alfa).
- **Tempo limite:** um despachante entrega um pedido por vez à thread STA e espera no máximo 2 s. Se uma extensão de shell travar, a thread é abandonada, o pedido volta sem ícone (e não é repetido) e uma thread nova atende o resto. Coberto por teste com um extrator que trava.
- **Sem dependência nova:** o `RenderImage` do GPUI exigiria o crate `image` como dependência direta. Em vez disso, o `fx-shell` codifica o ícone num PNG mínimo (blocos sem compressão) e a UI entrega os bytes ao GPUI (`Image::from_bytes`), que decodifica fora da thread de UI com o `image` que ele já usa.
- **Cache:** do app inteiro (todas as janelas e abas), por chave. A lista pede só as linhas desenhadas e reserva o espaço do ícone desde o primeiro quadro. `FX_ICONS=0` desliga, para comparar medições.
- **Medido (flat-100k):** aba nova com o fx carregado (P2 b, D-013): 9,6 ms com ícones, 10,1 ms sem. Rolagem: 0 quadros perdidos em 3.999 com ícones. Processo novo, do pedido da pasta ao 1º quadro: ~40 ms com ícones, ~38 ms sem. Era 24 ms na Fase 0: a diferença vem do primeiro quadro de um processo novo, que agora rasteriza os glifos da lateral, da barra de caminho e da fonte de ícones, antes do primeiro lote de arquivos.

## D-019 — Vista em grade e miniaturas (T1.7)
- **Data:** 2026-11-21 · **Status:** aceita
- **Miniaturas:** `IShellItemImageFactory::GetImage` (o mesmo caminho do Explorador, que usa o cache de miniaturas do Windows), numa thread STA em modo de fundo (`THREAD_MODE_BACKGROUND_BEGIN`: CPU, disco e memória com prioridade baixa). Primeira passada com `SIIGBF_INCACHEONLY`; o que não estiver no cache é extraído depois, com tempo limite de 5 s por item (thread travada é abandonada). Sem `SIIGBF_THUMBNAILONLY`: pastas e arquivos sem prévia recebem o ícone grande. Reduzidas a no máximo 96 px.
- **Cancelamento:** a grade informa a cada quadro os itens visíveis; a fila passa a ser exatamente essa lista, e o que saiu da tela sem ter começado é descartado.
- **Memória:** a UI guarda no máximo 400 miniaturas e, ao descartar, libera a cópia decodificada no GPUI (`ImageSource::remove_asset`). A textura na GPU não tem API pública para ser liberada (ficaria para um patch no GPUI). Medido: 85,6 MB privados com a grade aberta na `flat-10k-media`, 102,9 MB depois de rolar as 10 mil miniaturas. **Risco para o portão:** a P8 (≤ 80 MB parado) precisa ser remedida na T1.18.
- **Medido (`fx-bench grid`, `flat-10k-media`):** grade numa aba nova, P2 (b): mediana 9,3 ms, p95 11,0 ms. Rolagem com miniaturas chegando: 0 quadros perdidos em 3.999.
- **Teclas:** Ctrl+Shift+2 grade, Ctrl+Shift+6 detalhes (como no Explorador), mais o botão ao lado da barra de caminho; na grade, ←/→ andam um bloco e ↑/↓ uma linha. `FX_VIEW=grid` abre em grade (benchmarks).

## D-020 — Abrir arquivos (T1.9)
- **Data:** 2026-11-21 · **Status:** aceita
- **Decisão:** cada ação de abrir roda numa thread própria, com COM STA, e chama `ShellExecuteExW` (`SEE_MASK_NOASYNC`, porque a thread termina logo depois). O resultado volta por canal; um erro aparece na barra de status. Assim nem um app lento para iniciar nem um handler do shell travado seguram a UI (teste com um iniciador que demora 2 s).
- **Teclas:** Enter e duplo clique abrem o arquivo (ou os selecionados, até 32 de uma vez; pasta continua navegando), Shift+Enter abre "Abrir com…" (verbo `openas`), Ctrl+` abre o Windows Terminal na pasta (`wt -d`), ou o PowerShell se o Terminal não existir. Em teclados ABNT2, Ctrl+' também abre o terminal.
- **Feature nova do `windows`** (sem crate novo): `Win32_System_Registry`, exigida pela struct `SHELLEXECUTEINFOW` (campo `hkeyClass`). Nenhuma chamada ao registro é feita.

## D-021 — Acessibilidade com AccessKit (T1.10)
- **Data:** 2026-11-21 · **Status:** aceita
- **Dependências novas:** `accesskit` =0.25.2 e `accesskit_windows` =0.35.2 (MIT OR Apache-2.0; previstas na D-008 e na Fase 1; puxam `accesskit_consumer` 0.39.2). Também `raw-window-handle` =0.6.2 no `fx-shell`, **fora da lista prevista da Fase 1**: é a única forma pública de obter o HWND de uma janela do GPUI (trait `HasWindowHandle`, que o GPUI não reexporta). O GPUI já compila esse crate nessa versão, então nada novo é baixado. Alternativas descartadas: patch no GPUI para reexportar, ou achar a janela por título com `EnumThreadWindows` (frágil).
- **Arquitetura:** o `fx-shell` tem o adaptador (`SubclassingAdapter`, instalado com a janela ainda oculta, `show: false`, e só então mostrada). A `fx-ui` monta a árvore com o `accesskit` (neutro): janela → caminho → lista "Arquivos" (com o total em `size_of_set`) → as linhas visíveis (com a posição, seleção e ações Focus/Click). Leitores de tela pedem a árvore de dentro do procedimento de janela, onde o estado do app não pode ser emprestado; por isso a UI guarda a última árvore completa e o adaptador entrega uma cópia.
- **Custo:** a árvore (umas 20 linhas) é montada a cada quadro da lista e só é enviada quando há leitor de tela. Medido com o adaptador ativo (`FX_BENCH_UIA=1`, que lê a árvore antes do teste): P4 p95 3,35 ms (81 setas), P7 0 quadros perdidos em 4.000; sem ele, p95 3,81 ms e 0 em 4.000.

## D-022 — Lixeira, renomear, nova pasta e desfazer (T1.12)
- **Data:** 2026-11-21 · **Status:** aceita
- **Lixeira:** `IFileOperation` com `FOFX_RECYCLEONDELETE` (o motor do próprio Explorador). Antes de enviar, o caminho é **canonizado** (resolve `subst` e junções) e a unidade precisa ter Lixeira (`SHQueryRecycleBinW`); sem Lixeira, nada é excluído e o fx orienta usar Shift+Delete. **Motivo:** num teste no app pela unidade `subst` X:, o Windows excluiu um arquivo de teste de vez, sem aviso (o X: não tem Lixeira e o `FOF_NOCONFIRMATION` suprimiu o aviso). Era um arquivo do próprio teste em `fx-fixtures`; nenhum dado do usuário foi afetado. A correção foi verificada no mesmo cenário.
- **Desfazer exclusão:** o item é localizado na `$Recycle.Bin` da conta pelo arquivo `$I` (caminho original, aceito também o canônico, e a data) e o `$R` volta ao lugar com `MoveFileExW` sem substituir (nome ocupado vira "manter os dois"); o `$I` é removido, como faz o "Restaurar" do Windows. Item enviado que não for localizado aparece no aviso como "não localizado (o Windows pode tê-lo excluído de vez)", nunca como desfazível.
- **Desfazer em geral:** `fx_ops::undo_of` calcula o trabalho reverso a partir do que o trabalho realmente fez. Desfazer uma cópia manda as cópias para a Lixeira (não exclui de vez); desfazer mover devolve aos nomes originais; renomear volta o nome; nova pasta vai para a Lixeira; exclusão permanente não tem desfazer. Refazer roda o trabalho original de novo. Histórico de 100 entradas, do app inteiro.
- **Proteção (SAFETY.md §2.7):** `system_guard` recusa, sem confirmação extra, excluir/mover/renomear raiz de unidade e qualquer operação dentro de `%SystemRoot%` e das pastas de programas. A UI pergunta e reenvia com `confirm_system`; o motor confere de novo.

## D-023 — Área de transferência de arquivos (T1.14)
- **Data:** 2026-11-21 · **Status:** aceita
- **Decisão:** o formato do Explorador: `CF_HDROP` (bloco `DROPFILES` com os caminhos em UTF-16) mais "Preferred DropEffect" (1 = copiar, 2 = recortar). Codificação e decodificação são funções puras testadas (também o formato ANSI de apps antigos). Ler e gravar a área de transferência roda fora da thread de UI (outro app pode segurá-la aberta; há novas tentativas por ~200 ms). Colar vira um trabalho do motor (conflitos perguntados, desfazer registrado); colar algo recortado esvazia a área de transferência só depois que a movimentação termina bem.
- **Features novas do `windows`** (sem crate novo): `Win32_System_DataExchange` (área de transferência) e `Win32_System_Memory` (`GlobalAlloc`).
- **Teste com a área de transferência real:** marcado `#[ignore]`, porque sobrescreve a do usuário.

## D-024 — Arrastar e soltar (T1.15)
- **Data:** 2026-11-21 · **Status:** aceita
- **Soltar no fx:** pelo `IDropTarget` que o próprio GPUI registra (entrega os caminhos como `ExternalPaths`). Soltar na lista leva para a pasta aberta; numa linha de pasta, para dentro dela. Vira um trabalho do motor (conflitos perguntados, desfazer). O efeito segue o Explorador: mesma unidade move, outra copia, Shift move, Ctrl copia. As teclas são lidas do Windows no momento de soltar (`GetAsyncKeyState`), porque a janela do GPUI não recebe eventos de teclado enquanto outro app conduz o arraste. **Limitação:** o GPUI sempre responde "copiar" ao app de origem, então o ícone do cursor mostra "copiar" mesmo quando o fx vai mover (a operação feita é a certa).
- **Arrastar para fora:** `SHDoDragDrop` com o `IDataObject` do próprio shell (via `IShellItemArray::BindToHandler(BHID_DataObject)`), sem implementar COM: o destino recebe exatamente o que o Explorador daria e executa a operação ele mesmo (por isso esse lado não passa pelo desfazer do fx). Começa após 6 px de movimento; roda numa tarefa sem estado do app emprestado, porque o laço modal do OLE despacha mensagens da janela.
- **Patch novo no GPUI:** o `DragEnter` lia o `HGLOBAL` do `CF_HDROP` como ponteiro para um `HDROP` e derrubava o processo (violação de acesso) com os data objects do shell — o que incluiria qualquer arraste vindo do Explorador. Corrigido em `third_party/gpui` (`FX-PATCH.md`, `fx.patch`).
- **Feature nova do `windows`:** `Win32_UI_Input_KeyboardAndMouse` (`GetAsyncKeyState`).
- **Diagnóstico:** o fx grava pânicos em `%TEMP%\fx-crash-<pid>.log` (app de janela não tem console).

## D-025 — Menu de contexto do shell (T1.16)
- **Data:** 2026-11-21 · **Status:** aceita
- **Decisão:** o menu clássico do shell (`IShellFolder::GetUIObjectOf` → `IContextMenu`, ou `CreateViewObject` para o fundo da pasta), mostrado por uma janela oculta numa thread STA com OLE, com `IContextMenu3::HandleMenuMsg2` para os submenus desenhados pelo shell ("Abrir com", "Enviar para"). Itens do fx no topo ("Abrir no Terminal aqui", "Copiar caminho"). Os verbos `delete`, `rename`, `copy`, `cut` e `paste` voltam para o fx executar pelo próprio motor (Lixeira, conflitos, desfazer); os demais o shell executa (`InvokeCommand`).
- **Tempo limite:** só enquanto o menu é montado (2 s); uma extensão que trava faz o menu ficar "indisponível" e a thread é trocada (teste com extensão falsa). Com o menu na tela não há limite. A UI nunca espera: tudo roda fora da thread de UI.
- **Aquecimento:** ao abrir, o fx monta (sem mostrar) o menu da pasta pessoal, carregando COM, OLE e extensões comuns: o primeiro menu caiu de ~460 para ~180 ms.
- **Medido (`fx-bench menu`, 20 menus na `flat-100k`, PC com PowerToys, 7-Zip, Defender e outros):** mediana 64 ms, **p95 259 ms (meta de 150 ms não atingida neste PC)**. O tempo é a montagem pelo shell (`QueryContextMenu` das extensões), não espera do fx: a primeira abertura por tipo de arquivo é a lenta (`.jpg` 375 ms, `.exe` 217 ms) e na segunda passada todos ficam em 41–69 ms. A meta fala em "sem extensões lentas", o que não dá para isolar aqui sem desinstalar extensões do usuário. Risco anotado para o portão.
- **Achado de método:** um processo de teste que ficou aberto fez o Windows reutilizar a imagem antiga do `fx.exe` lida por `\\wsl.localhost` em processos novos (medições com binário velho). Antes de medir, garantir que nenhum `fx.exe` antigo esteja rodando.
