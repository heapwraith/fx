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

## D-026 — Persistência (T1.17)
- **Data:** 2026-11-21 · **Status:** aceita
- **Dependências novas** (previstas na Fase 1): `serde` =1.0.229 (derive), `toml` =0.8.23 (já estavam no grafo pelo GPUI) e `rusqlite` =0.37.0 com `bundled` (SQLite embutido, domínio público; compila pela toolchain cruzada). Puxam `libsqlite3-sys`, `hashlink`, `fallible-iterator`, `fallible-streaming-iterator` (MIT/Apache-2.0).
- **Configurações:** `%APPDATA%\fx\settings.toml` (densidade, barra lateral, restaurar sessão), chaves em português, padrões para o que faltar; gravado de forma atômica (temporário + renomear).
- **Estado:** `%LOCALAPPDATA%\fx\state.db` (SQLite, WAL): janelas (posição), abas (pasta, rolagem, grade, seleção, cursor, histórico) e Acesso rápido; cada gravação é uma transação. A UI monta o estado a cada 2 s e uma thread grava se mudou; fechar a última janela grava na hora. Abrir sem pasta restaura a sessão; com pasta, abre só ela.
- **Testes não sobrescrevem a sessão do usuário:** com `FX_TRACE` (benchmarks) ou `FX_NO_STATE=1` nada é lido nem gravado; `FX_DATA_DIR` troca a pasta de dados (verificação no app).
- **Teste do critério:** um processo filho grava estados sem parar e é morto (`TerminateProcess`) 5 vezes: o banco passa no `integrity_check` e devolve sempre o último estado completo. No app: abas, pastas, seleção, cursor e histórico voltaram depois de matar o processo.
- **Ainda pendente da D-013:** fechar a última janela continua encerrando o processo (residente sem janela exige ícone na bandeja ou opção de sair; fica para a Fase 2).

## D-027 — Identidade e acabamento visual (T1.19, checkup)
- **Data:** 2026-11-21 · **Status:** aceita
- **Cor de destaque própria:** verde-azulado (`#0B7A75` no claro, `#3DD6C6` no escuro) em vez do azul padrão do Windows, para a seleção, o progresso, os botões principais e as barras dos discos. Passa nos testes de contraste (3:1 contra a lista; 4,5:1 para o texto do botão principal, teste novo).
- **Velocidade visível:** o status mostra quanto levou para ter a lista inteira ordenada ("100.000 itens · lida em 9 ms"), medido do início da leitura até a ordenação. Na primeira pasta de um processo novo o número inclui a disputa com a partida do app (ex.: pasta vazia em ~49 ms); dentro do app ele mede só a leitura.
- **Horário local** nas datas, com as regras do Windows para aquela data (horário de verão incluído), como no Explorador: `SystemTimeToTzSpecificLocalTimeEx` em `fx-shell` (`utc_offset_at`). **Feature nova do `windows`:** `Win32_System_Time`. A coluna passa a se chamar "Modificado em".
- **Estados de borda:** erro de leitura e pasta vazia aparecem no meio da lista, em português (sem permissão, pasta inexistente, unidade não pronta, rede indisponível, código do Windows nos demais), e o status fica curto, sem espremer a barra de caminho. Filtro sem resultado também avisa.
- **Contagens** com separador de milhar ("100.000 itens") em toda a interface e na árvore de acessibilidade.
- **Abas:** largura mínima de 112 px; quando não cabem, a faixa rola, e a aba ativa é trazida à vista. O "×" aparece só na aba ativa ou sob o mouse.
- **Conflito:** o motor (`Event::Ask`) passa tamanho e data dos dois lados, lidos na thread do trabalho; o diálogo mostra "Existente" e "Novo" e marca o mais recente.

## D-028 — Painel de histórico de operações (T1.19, checkup)
- **Data:** 2026-11-21 · **Status:** aceita
- **Decisão:** um botão de relógio na faixa de abas (e Ctrl+Shift+H) abre um painel com todas as operações da sessão que podem ser desfeitas, da mais nova para a mais antiga, com hora e uma frase ("Renomeou "b.txt" para "q.txt"", "Enviou 3 itens para a Lixeira"). Cada uma tem o próprio "Desfazer", que reverte só aquela, mesmo que não seja a última; as mais novas ficam como estão. Esc ou clicar fora fecha.
- **Segurança:** a reversão é um trabalho comum do motor (sem sobrescrever, SAFETY.md §2.3). Se algo mudou depois (o item foi renomeado ou movido), a reversão falha e avisa; a entrada volta ao histórico. `UndoStack` ganhou `undo_entries` e `take_undo_at` (com teste); Ctrl+Z continua desfazendo a mais recente.
- **Verificado na janela:** criar pasta, renomear arquivo, abrir o painel e desfazer só a criação: a pasta saiu (para a Lixeira, como todo desfazer de "nova pasta") e a renomeação continuou.

## D-029 — Ordenar por coluna, coluna Tipo e larguras (T1.20)
- **Data:** 2026-11-21 · **Status:** aceita
- **Ordenação:** clicar no cabeçalho ordena por nome, tipo, tamanho ou data; clicar de novo inverte. O primeiro clique em data e tamanho já vem do maior/mais novo para o menor (o que se procura quase sempre); nome e tipo começam de A a Z. Pastas ficam sempre antes. Uma seta no cabeçalho mostra a coluna e a direção.
- **Fora da thread de UI:** a nova ordem é calculada numa thread de fundo sobre uma cópia barata (`Arc`) da lista e aplicada com `set_sorted_order`. Se entraram ou saíram itens nesse meio-tempo, ela é recalculada; um resultado velho (cliques seguidos) é descartado. Seleção e cursor são guardados por entrada, então sobrevivem (teste em `fx-core`); depois de reordenar, o cursor é trazido à vista.
- **Custo medido** (`fx-bench list`, 100 mil, thread de fundo): tamanho 9,5 ms, data 8,9 ms, nome 39,5 ms, tipo 69 ms (comparação natural da extensão).
- **Coluna Tipo:** "Pasta de arquivos", "Arquivo JPG" ou "Arquivo". Os nomes amigáveis do registro ("Documento de Texto") pedem o shell por extensão; ficam para depois, com cache.
- **Larguras:** arrastar a borda esquerda de Tipo, Tamanho ou Data ajusta a coluna (56 a 480 px); o nome ocupa o resto. Em lista estreita somem nesta ordem: tipo, data, tamanho.
- **Por aba, persistido:** ordenação e larguras vão numa tabela nova do `state.db` (`tab_view`), para bancos antigos abrirem sem migração.

## D-030 — Ocultos, Propriedades e menu Exibir (T1.21)
- **Data:** 2026-11-21 · **Status:** aceita
- **Ocultos:** itens com atributo oculto ou de sistema ficam fora da lista por padrão (`FileList::visible`, combinado com o filtro por nome, sem cópia quando não há nada a esconder). Ctrl+H ou o menu Exibir alternam para a janela inteira; a escolha vai para `settings.toml` (`mostrar_ocultos`). Quando visíveis, aparecem esmaecidos (55%). O status conta os escondidos ("1.001 itens · 12 ocultos"); uma pasta só com ocultos avisa no meio da lista.
- **Segurança:** esconder tira os ocultos da seleção e do cursor; a seleção restaurada da sessão passa pela mesma limpeza; e `targets()` não devolve nada se o cursor não está numa linha visível. Nenhuma operação age num item que não se vê.
- **Propriedades:** Alt+Enter e o item "Propriedades" do menu do shell (o verbo `properties` agora volta para o fx, como excluir e renomear) abrem a janela de Propriedades do Windows numa thread de lançamento: um item pelo verbo `properties` (`SEE_MASK_INVOKEIDLIST`), vários pelo `SHMultiFileProperties` com o data object do shell (o mesmo do arrastar, agora uma função comum). Sem seleção, as Propriedades da pasta atual.
- **Menu Exibir:** botão ao lado do de grade com "Mostrar itens ocultos", "Ordenar por" (com crescente/decrescente) e "Densidade", com os atalhos ao lado: nada fica só no teclado. Densidade e ocultos valem para a janela (a aba emite um `ViewEvent` que a `TabsView` aplica a todas). Com filtro digitado, uma lupa aparece antes do texto do status.

## D-031 — Quick Look (T1.22)
- **Data:** 2026-11-21 · **Status:** aceita
- **Forma:** um painel grande sobre a lista (não uma janela à parte do sistema), com nome, tipo e tamanho no topo. Espaço abre e fecha (com um filtro sendo digitado, Espaço continua indo para o filtro); Esc fecha; clicar fora fecha. As setas continuam movendo o cursor e a prévia acompanha.
- **Carregamento fora da UI:** cada prévia roda numa thread própria (`fx-preview`) e devolve o conteúdo por canal; um pedido mais novo descarta o resultado do anterior (bilhete). Uma extensão de miniatura travada só deixa a thread para trás.
- **Imagens** (PNG, JPEG, GIF, WebP, BMP, TIFF, até 64 MB): decodificadas pelo decodificador do próprio GPUI (`Image::to_image_data`) na thread da prévia, já prontas para a GPU; a cópia na GPU é liberada (`drop_image`) quando a prévia muda ou fecha. O `SvgRenderer` que o GPUI exige não tem tipo público: vai para a thread dentro de uma closure. Sem dependência nova (o crate `image` não foi adicionado).
- **Texto e código** (lista de extensões, e arquivos sem extensão farejados): os primeiros 256 KB, UTF-8 (com ou sem BOM) ou UTF-16 com BOM; binário (byte NUL nos primeiros 8 KB) cai para a miniatura. Linhas numa lista virtualizada (só as visíveis são desenhadas), cortadas em 2.000 caracteres.
- **Demais tipos e pastas:** miniatura grande do shell (512 px, `IShellItemImageFactory`, o mesmo extrator da grade com o tamanho como parâmetro), ou o ícone grande.
- **Medido:** prévia de texto de um arquivo de 1 MB, release: **1,5–1,8 ms** para ler e preparar. **Imagem de 10 MB ainda não medida:** o decodificador do GPUI só existe dentro do app, e a medida (`input_preview_ms` no `FX_TRACE`, da tecla até o quadro) pede a janela.
- **Fase 2:** vídeo, áudio, PDF e preview handlers de terceiros (`fx-preview` isolado em processo).

## D-032 — Busca no disco (T1.23)
- **Data:** 2026-11-21 · **Status:** aceita
- **Painel:** Ctrl+F abre um painel de busca sobre a aba: campo no topo, resultados com nome e pasta, status embaixo. Setas escolhem, Enter abre (pasta abre na aba, arquivo no app padrão), Ctrl+Enter mostra o item selecionado na pasta dele, Esc ou clicar fora fecha. Cada tecla cancela a busca anterior (bandeira atômica) e começa outra; um resultado atrasado de uma busca antiga é descartado (bilhete).
- **Sem elevação (o normal até o `fx-indexd`):** `fx_fs::search_tree` percorre a pasta atual e as subpastas em largura numa thread, com a mesma enumeração das pastas, "contém" sem maiúsculas nem acentos, lotes a cada 8 ms, até 2.000 resultados. Links e junções aparecem mas não são percorridos (sem laços); pastas sem acesso são puladas e contadas no status.
- **Elevado:** `fx-shell::is_elevated` (token do processo). O fx lê a MFT do volume uma vez numa thread (`fx-index`, alguns segundos) e guarda o índice para o processo inteiro; daí em diante a busca usa o índice (disco inteiro, os 200 melhores por pontuação difusa). Enquanto o índice não fica pronto, vale a busca por pastas.
- **Medido:** varrer todas as fixtures (~112 mil itens, 108 pastas), release: ~190–200 ms; uma busca com 2.296 resultados os entregou em 11–29 lotes. Teste: nas fixtures, a busca acha exatamente o que uma varredura independente com `std::fs` acha (3 consultas, com acento e maiúsculas).
- **Falta:** P4 com resultados chegando (`input_search_ms`), que pede a janela.

## D-033 — Paleta de comandos (T1.24)
- **Data:** 2026-11-21 · **Status:** aceita
- **Abrir:** Ctrl+K ou Ctrl+Shift+P, na janela. Campo no topo; ↑↓ escolhe (dá a volta), Enter executa, Esc ou clicar fora fecha. Com a paleta aberta, todas as teclas ficam com ela.
- **Itens:** os comandos do fx com o atalho ao lado (`fx-ui/src/commands.rs`, 36 comandos), o Acesso rápido e as pastas recentes (as outras abas e o histórico de todas, sem repetir, até 15). Sem texto: 5 recentes, Acesso rápido, depois os comandos.
- **Busca difusa** (`fx-core::fuzzy`): as letras na ordem, sem maiúsculas nem acentos; pontua sequências, começos de palavra e casamento cedo ("np" acha "Nova pasta"). Uma pasta também é achada pelo caminho completo, abaixo das que casam pelo nome.
- **Um caminho só:** cada comando com atalho executa o próprio atalho na janela ou na aba ativa (`FolderView::press` monta o `KeyDownEvent` e chama o mesmo tratador do teclado), então paleta e teclado nunca divergem. Sem atalho: alternar grade e ordenar por coluna. Testes: rótulos únicos, todo atalho é válido e nenhum se repete.
- **Atalho novo:** Ctrl+Shift+C copia o caminho dos itens (como "Copiar como caminho" do Explorador), também na paleta.
- **Medido:** filtrar 336 itens, release: pior tecla 0,2–0,35 ms (meta: 16 ms por tecla).
- **Falta:** ver na janela e medir a tecla até o quadro (`input_palette_ms`).

## D-034 — Conferência na janela de T1.20–T1.24 e correções
- **Data:** 2026-11-21 · **Status:** aceita
- **Como:** `bench/drive.ps1` manda teclas, cliques e tira capturas de uma janela do fx; `bench/windows-of.ps1` lista as janelas de um processo (para ver se um diálogo abriu). `fx-bench features` mede com entrada real: reordenar pela paleta e setas logo depois (flat-100k), digitar na busca com resultados chegando (raiz das fixtures) e Quick Look em fotos de ~10 MB (`bench/preview-fixture.ps1` cria `fx-fixtures\preview`).
- **Propriedades:** não abriam. (1) No `match` do teclado, `_ if m.alt` vinha antes de `"enter" if only_alt`. (2) A folha de `ShellExecuteEx("properties")` e a de `SHMultiFileProperties` usam o objeto de dados no apartamento COM que as criou; a thread de lançamento acaba logo e a folha nunca aparece (testado: com a thread viva bombeando mensagens, aparece). Agora: 1 item por `SHObjectProperties` (só um caminho, não depende da thread); vários itens numa thread STA única e permanente (`fx-shell/src/windows/sheets.rs`), parada em `GetMessageW` quando ociosa.
- **Quick Look de fotos grandes:** decodificar um JPEG de 12 MP inteiro custava 133–150 ms (meta 100). JPEG e TIFF acima de 2 MB vêm da miniatura do shell em até 1920 px (o WIC decodifica o JPEG já reduzido): ~45 ms. PNG continua com o decodificador do GPUI (10 MB: ~40 ms; pelo shell seria ~55). A miniatura chega como BMP de 32 bits (`encode_bmp`: cabeçalho + pixels, sem o CRC bit a bit do PNG). A primeira decodificação pelo WIC no processo custa ~50 ms a mais; quando o cursor para numa foto grande pela primeira vez, ela é decodificada uma vez em segundo plano e descartada (só quem vê fotos paga a memória do WIC).
- **Tipos sem miniatura** (ex.: `.bin`): o ícone era esticado até ocupar o painel. Agora pede só miniatura (`SIIGBF_THUMBNAILONLY`) e, sem ela, mostra o ícone a 128 px com a legenda ("Enter abre a pasta." para pastas).
- **Ctrl+Shift+2/6 (grade/lista) nunca funcionaram:** com Shift, o GPUI troca o dígito pelo caractere da camada Shift ("@"; no ABNT2 o 6 é tecla morta) e tira o Shift dos modificadores. O fx consulta o estado do teclado (`fx_shell::shifted_digit`, `GetKeyState`) para saber o dígito.
- **Layout:** títulos de Tipo/Tamanho/Data 11 px à esquerda dos valores (o espaçador da barra de rolagem levava o `gap`) — corrigido; o status com largura do próprio texto não ganhava reticências e empurrava a barra de caminho — agora divide o espaço com ela (`flex_1`, até 400 px) e a seleção vem logo após a contagem, para não ser a parte cortada; paleta e busca realçam a linha inteira; a paleta usa os nomes da lateral para pastas conhecidas.
- **Não é defeito:** "comentÃ¡rio" numa prévia veio do arquivo de teste (o PowerShell 5 leu o script como ANSI); UTF-8 com e sem BOM aparecem certos. Com a caixa de renomear aberta, ela fica com as teclas (Ctrl+Shift+H não abre o histórico), como no Explorador.
- **Fica para depois:** a primeira migalha cortada no meio quando o caminho não cabe (ex.: "ixtures"); o Explorador recolhe as do início num botão "…".

## D-035 — Portão da Fase 1 sem as 2 semanas de uso
- **Data:** 2026-11-21 · **Status:** aceita (decisão do usuário)
- **Decisão:** o critério "2 semanas como gerenciador principal" fica dispensado. O usuário quer o fx completo primeiro; vai usá-lo e melhorá-lo depois. Os outros critérios passaram (P1–P4, P7, nenhuma perda de dados; `bench/results/gate-1.md`).
- **Consequência:** a Fase 2 está em `docs/phases/PHASE-2.md`, aprovada pelo usuário em 2026-11-21. Problemas que só o uso diário mostraria podem aparecer mais tarde; os testes de falha do motor de operações (T2.4) vêm antes da beta pública para compensar.

## D-036 — Ícone e recursos do executável (T2.1)
- **Data:** 2026-11-21 · **Status:** aceita
- **Ícone:** desenho próprio, sem marca de terceiros: pasta na cor de destaque do fx (teal) com uma seta dupla »» de velocidade; em 16–24 px só a pasta, para ficar nítido. A fonte é `tools/make-icon.py` (Pillow, desenhado a 8× e reduzido), que gera `crates/fx-ui/resources/fx.ico` com 16, 20, 24, 32, 40, 48, 64, 96, 128 e 256 px. Para mudar o ícone, edite o script e rode-o de novo.
- **Recursos:** `crates/fx-ui/build.rs` gera um `.rc` (UTF-8) com o ícone de id 1 e o VERSIONINFO (descrição, versão do `Cargo.toml`, licença, idioma pt-BR) e o compila com `embed-resource`. O GPUI já carrega o ícone de id 1 do executável para a classe da janela (barra de título, barra de tarefas, Alt+Tab) e já embute o manifesto (DPI por monitor, Common Controls 6), então não há conflito.
- **Dependência:** `embed-resource = "=3.0.12"` como dependência de build do `fx-ui`. Não é crate novo: é a mesma versão que o `build.rs` do GPUI já usa (estava no `Cargo.lock`). Usa o `llvm-rc` no WSL e o `rc.exe` do Windows SDK no CI.

## D-037 — Pipeline no GitHub e versionamento (T2.2)
- **Data:** 2026-11-21 · **Status:** aceita
- **CI:** `.github/workflows/ci.yml`, em `windows-latest`, compila nativo com MSVC (aqui no WSL continua o `cargo xwin`, D-010). A cada push no `main` e a cada PR: `cargo fmt --check`, clippy com `-D warnings`, `cargo test --workspace` (com `FX_NO_STATE=1`) e build release; o `fx.exe` fica como artefato por 14 dias. Cache com `Swatinem/rust-cache`; toolchain do `rust-toolchain.toml`. Primeira execução: 17 min (os testes do `fx-index` levam ~170 s em debug no runner).
- **Versão:** semver; enquanto 0.x, cada fase sobe o número do meio (Fase 1 = 0.1.0, beta da Fase 2 = 0.2.0-beta). Uma versão só, a do `[workspace.package]`. O `build.rs` do `fx-ui` grava `versão+commit` (com `-dirty` se houver alteração local) no VERSIONINFO (`ProductVersion`) e em `crate::VERSION`, mostrado pelo comando "Sobre o fx (versão)" da paleta.
- **Release:** a tag `vX.Y.Z` dispara o job `release`, que falha se a tag não bater com o `Cargo.toml` ou se o `CHANGELOG.md` não tiver a seção da versão; monta `fx-X.Y.Z-windows-x64.zip` (exe, licenças, README, CHANGELOG) e cria um release em **rascunho** (pré-lançamento enquanto 0.x) com as notas da seção. Publicar o rascunho é decisão do usuário.
- **Pendente:** as actions `checkout@v4` e `upload-artifact@v4` usam Node 20, que o GitHub está aposentando (funciona com aviso); atualizar quando houver versões novas estáveis.

## D-038 — Migalhas recolhidas, travamento do texto truncado e P8 (T2.3)
- **Data:** 2026-11-21 · **Status:** aceita
- **Travamento:** o fx caía na partida em certas larguras de janela (`panicked at direct_write.rs:595: end byte index … is inside '…'`), inclusive o `fx.exe` do CI da 0.1.0. Causa: no patch da T1.19 a medição do texto roda de novo para outra largura, e o truncamento encurtava os `runs` no próprio lugar; o texto original medido com `runs` já encurtados era fatiado no meio do "…" (3 bytes em UTF-8). Correção no patch do GPUI: cada medição trunca uma cópia (`FX-PATCH.md`, `fx.patch`). Três partidas seguidas no mesmo cenário abriram normalmente.
- **Migalhas:** quando não cabem, as do início viram um botão "…" (como o "«" do Explorador) com um menu das pastas escondidas, da raiz para baixo; a pasta atual sempre aparece e nenhuma migalha é cortada no meio. A largura de cada migalha vem do sistema de texto do GPUI (`shape_line`, com cache); a área disponível é medida por um `canvas` e usada no quadro seguinte (`first_shown`, com testes). O status passou a ter a largura do próprio texto (até 400 px), também medida, em vez de dividir o espaço com a barra.
- **P8:** `fx-bench memory` (5 processos, memória privada 6 s após abrir as fixtures): **85,3 MB** (meta 80). Medido por partes com variáveis de ambiente: ícones de arquivo +6,4 MB (`FX_ICONS=0`: 78,9 MB), Mica 0. Num teste isolado, o primeiro `SHGetFileInfoW` custa 3,2 MB (cache de ícones do shell), `IShellItemImageFactory` 6,3 MB e extrair o ícone do recurso (`PrivateExtractIconsW`) 0,1 MB. Mas resolver o ícone pelo registro não reproduz o Explorador (ex.: `.rs` sem ProgID aparece com o ícone do VS Code por regras internas do shell; a pasta aponta para `shell32.dll,3`), e pôr o shell em outro processo só esconderia a memória. **Opções para o usuário:** (a) rever a meta para ~88 MB, registrando que o custo é o cache de ícones do próprio Windows, que o Explorador também paga; (b) ícones pelo registro com volta ao shell nos casos difíceis, aceitando diferenças visuais em alguns tipos; (c) continuar procurando os 5 MB em outras partes (lateral +3,3 MB, AccessKit +1,5 MB).
- **Decisão (2026-11-22, delegada pelo usuário):** opção (a). A P8 passa a ≤ 88 MB: o excedente é o cache de ícones do Windows, que o Explorador também carrega, e trocar os ícones por uma resolução própria mostraria ícones diferentes dos do sistema. Medido hoje: 85,3 MB. A meta volta a ser revista se algo novo subir a memória parada.

## D-039 — Testes de falha do motor e diário de temporários (T2.4)
- **Data:** 2026-11-22 · **Status:** aceita
- **Lacuna achada:** cancelar ou falhar já apagava o temporário (`.nome.fx-PID-JOB.tmp`), mas se o processo morresse no meio da cópia ninguém o apagava, contra a SAFETY.md §2.5 ("não deixa lixo").
- **Diário** (`fx-ops/src/journal.rs`): antes de criar cada temporário, o caminho é anotado no diário do processo (`%LOCALAPPDATA%\fx\ops\pending-<pid>-<ns>.log`, UTF-16 em hex para não perder nenhum nome). O arquivo fica aberto sem compartilhamento enquanto o processo vive. Na partida, `Engine::start_with_journal` chama `recover`: abre os diários que consegue (o processo deles acabou), apaga os temporários listados que ainda existem e depois o diário. Só nomes no formato exato do temporário são apagados, então um diário nunca leva o fx a apagar arquivo do usuário (teste). Sem pasta de dados, o motor roda sem diário. Se anotar no diário falhar, a cópia não começa.
- **Testes** (`fx-ops/src/tests.rs`, SAFETY.md §3): disco cheio é simulado (gancho de teste que grava parte do temporário e devolve `ERROR_DISK_FULL`), porque um volume pequeno de verdade exige administrador; arquivo em uso com `share_mode(0)`; somente leitura; processo morto com um processo filho real (o próprio executável de teste, com a cópia desacelerada) e `TerminateProcess`; > 4 GiB com `set_len` e marcas no começo e no fim. Unicode, caminhos > 260, 0 bytes e cancelamento já tinham teste (T1.11).
- **Limites:** o diário não é gravado com `fsync`; numa queda de energia (não de processo), a última linha pode se perder e um temporário ficar para trás. O teste de disco cheio real (volume pequeno) fica para quando houver um ambiente com administrador.

## D-040 — Serviço de índice `fx-indexd` (T2.5)
- **Data:** 2026-11-22 · **Status:** aceita
- **O serviço:** `fx-indexd.exe` (subcomandos `install`, `uninstall`, `status`, `query`, `run` e `service`, este chamado pelo Windows). Lê a MFT de cada volume NTFS fixo (`fx-index`), com o diário USN aberto antes da leitura para não perder mudanças, e aplica o diário a cada 250 ms. Roda como LocalSystem (abrir o volume exige administrador), início automático atrasado.
- **Segurança:**
  - **instalação:** o `install` copia o executável para `%ProgramFiles%\fx\fx-indexd.exe`. Um serviço SYSTEM rodando de uma pasta do usuário seria uma elevação de privilégio para qualquer programa que trocasse o arquivo;
  - **pipe:** `\\.\pipe\fx-indexd`, só local (`PIPE_REJECT_REMOTE_CLIENTS`), com ACL apenas para SYSTEM e o SID de quem instalou (o usuário que aceitou o UAC), criado com `FILE_FLAG_FIRST_PIPE_INSTANCE` (ninguém toma o nome antes);
  - **cliente:** abre o pipe com `SECURITY_IDENTIFICATION`, para que um falso servidor não possa agir como o usuário;
  - **protocolo:** binário com limites fixos (consulta ≤ 1 KB, ≤ 1000 resultados), testado contra mensagens malformadas;
  - **privacidade:** os resultados dentro das pastas de perfil dos outros usuários (`ProfileList`) não são devolvidos.
- **Código:** o unsafe do Windows (pipe, SCM, volumes, SID, registro) fica em `fx-index/src/windows/{pipe,service,system}.rs`, já que o `fx-indexd` não pode ter unsafe (CLAUDE.md); a lógica do serviço (busca nos volumes, filtro de perfis) fica em `fx-indexd/src/engine.rs`, com testes. Features novas do crate `windows`: `Win32_System_Services` e `Win32_Security_Authorization` (sem crate novo).
- **No app:** comandos na paleta "Ativar/Desativar a busca no disco inteiro" rodam `fx-indexd.exe install|uninstall` com o verbo `runas` (UAC) e esperam o código de saída (`Launch::Elevated`). O Ctrl+F passa a usar o serviço na T2.6.
- **Medido** (elevado, `bench/indexd-test.ps1`): 744.649 itens (C: e D:) em 7,5 s; índice 71,3 MB = **95,7 MB/milhão** (P9 ≤ 150); arquivo novo aparece em 101–258 ms (≤ 1 s); busca 3,8 ms; desinstalação sem resto.
- **Pendente:** ~~o processo inteiro fica em ~142 MB~~ resolvido na T2.6 (D-041: a primeira mudança dobrava os buffers do índice). Uma conexão que não manda nada prende o servidor (só o próprio usuário consegue abrir o pipe); pôr tempo limite.

## D-041 — Busca completa: filtros, alcance e resultados (T2.6)
- **Data:** 2026-11-22 · **Status:** aceita
- **Filtros digitados com o nome** (`fx-core/src/search_filter.rs`): `tipo:` (imagem, vídeo, áudio, documento, código, compactado, pasta, arquivo), `ext:pdf,docx`, `tamanho:>10mb|<1kb|1mb..1gb` (potências de 1024, como o Explorador) e `data:hoje|ontem|semana|mês|ano|>2026-01-01|<…|a..b` (dia local). Chaves em inglês também valem. Palavras com chave conhecida e valor ilegível aparecem como "não entendi", em vez de serem ignoradas; outras palavras com dois-pontos (`C:`, `10:30`) continuam no nome. Escolhido o texto em vez de botões: é teclado primeiro e funciona igual na paleta, no serviço e no `fx-indexd query`.
- **Filtrar dentro da busca, nunca depois:** `fx_index::search_where` recebe um predicado que roda no laço da varredura (um nome filtrado nunca ocupa um dos 200 lugares) e devolve um bônus de pontuação. Sem nome, só com filtros, a varredura lista os aceitos com os mais recentes primeiro. A varredura de pasta tem o equivalente `fx_fs::search_tree_where` (o limite de 2000 conta só o que passa). O protocolo do serviço passou a `FXQ2`, com os filtros no pedido (limites fixos: ≤ 64 extensões de ≤ 16 bytes).
- **Alcance:** Tab alterna "Disco inteiro" (o `fx-indexd` quando roda; ou o índice próprio da MFT do volume atual quando o fx está elevado) e "Nesta pasta" (varredura). Ao abrir, o painel pergunta ao Gerenciador de Serviços em segundo plano e escolhe "Disco inteiro" se puder. Sem serviço, o Tab avisa como ativá-lo.
- **Resultados:** colunas Tamanho e Modificado; clicar no título ordena por nome (ordem natural), tamanho (maiores primeiro) ou data (mais recentes primeiro), e clicar de novo volta à relevância. Duplo clique abre; arrastar leva o item para fora (`drag_files`); botão direito abre o menu do Windows na pasta do resultado (`open_menu_in`).
- **Memória do índice vivo:** depois do `shrink_to_fit`, a primeira inserção dobrava a capacidade de todos os vetores (o índice do serviço ia de 71 para 139 MB, e o processo para 142 MB, assim que um arquivo era criado). Agora os vetores crescem 1/32 do tamanho (mínimo de 4.096 entradas). Medido no serviço com 200 arquivos criados e apagados: índice estável em **73,5 MB** para 744 mil itens (98,7 MB/milhão), processo **76,3 MB**. Teste: uma inserção depois da construção cresce menos de 1/16.
- **Medido:** P5 pelo serviço (pipe + busca nos volumes + filtros, até 1000 resultados) **p95 5,39 ms** em 243 teclas (`fx-bench search --service`); P4 digitando na busca p95 3,57 ms. Ativar e desativar o serviço pela paleta, com o UAC, conferidos na janela; a remoção não deixou nada.
- **`bench/drive.ps1`:** digita qualquer caractere do layout atual (`VkKeyScanW`), inclusive ":" e ">" no ABNT2.

## D-042 — Tamanho das pastas (T2.7)
- **Data:** 2026-11-22 · **Status:** aceita
- **Cálculo:** quando a pasta termina de carregar, uma thread de fundo (`fx-dir-sizes`) soma cada subpasta com `fx_fs::folder_size` (mesma enumeração da lista, sem seguir junctions e links, como a busca), na ordem em que aparecem na tela. Os totais chegam em lotes; enquanto não chegam, a coluna mostra "…". Sair da pasta ou recarregá-la cancela (flag atômica, a soma para entre pastas). O total é a soma dos tamanhos dos arquivos, o mesmo "Tamanho" das Propriedades do Windows (não o "tamanho em disco").
- **Cache da sessão:** os totais ficam guardados por caminho enquanto o fx estiver aberto: voltar a uma pasta mostra na hora, sem recontar. Não é invalidado por mudanças dentro das subpastas (o observador só vê a pasta atual); um total pode ficar desatualizado até reabrir o fx.
- **Ordenação:** `FileList::set_size` grava o total; com a coluna Tamanho ativa, a lista é reordenada (em segundo plano) quando todos chegam. As pastas continuam antes dos arquivos.
- **Custo:** pastas enormes (`C:\Windows`) levam segundos para somar, em segundo plano, sem afetar a tecla (P4 medido com p95 ≤ 3,5 ms).
- **Pelo serviço:** o pedido do protocolo passou a `FXQ3`, com o campo `sizes_of` (uma pasta absoluta): em vez de buscar, o `fx-indexd` devolve uma entrada por subpasta com o total. `Index::folder_totals` acha a pasta componente a componente (sem diferenciar maiúsculas) e soma numa passada sobre todos os arquivos, subindo pelos pais de cada um até encontrar uma subpasta: **~32 ms com 1 milhão de itens**, em qualquer profundidade (release). As pastas dentro do perfil de outros usuários ficam de fora. No app, a thread pergunta ao serviço primeiro; sem ele, o pipe falha na hora e entra a varredura. O que o serviço não conhece (outro volume, rede) também é varrido. Os totais do serviço estão sempre atuais (USN) e não usam o cache.

## D-043 — Renomear em lote (T2.8)
- **Data:** 2026-11-22 · **Status:** aceita
- **Regras** (`fx-core/src/batch_rename.rs`): em cada nome, na ordem da tela, buscar e substituir no nome sem a extensão (texto puro, ou expressão regular com grupos `$1`; com ou sem diferenciar maiúsculas), depois o modelo (`{nome}` = resultado até aqui, `{n}`/`{n:3}` = posição a partir de 1), depois as letras (manter, minúsculas, MAIÚSCULAS, Título). A extensão é preservada; arquivos que começam com ponto e nomes sem ponto são tratados inteiros.
- **Validação antes do disco:** nome vazio, caracteres proibidos (`<>:"/\|?*` e de controle), terminado em ponto ou espaço, longo demais (> 255), reservados (CON, PRN, AUX, NUL, COM1–9, LPT1–9); repetido dentro do lote (sem diferenciar maiúsculas); igual a um item da pasta fora do lote (ocultos inclusive). Um nome liberado por outro item do lote vale. Enter só aplica sem nenhum problema; a regex inválida aparece no rodapé.
- **Motor** (`Job::batch_rename`, `OpKind::BatchRename`): duas fases na mesma pasta: todos para nomes temporários (`.fx-ren-<pid>-<job>-<k>.tmp`), depois para os finais, sempre com o rename que não substitui. Isso resolve trocas e ciclos (a→b, b→c, c→a) e a troca só de maiúscula. Se qualquer passo falhar, o que já foi feito volta aos nomes originais e nada fica temporário (teste). O histórico guarda o lote inteiro: um Ctrl+Z reverte todos os nomes.
- **Painel:** três campos (Tab alterna; Ctrl+Backspace limpa o campo), Alt+R/Alt+M/Alt+C, prévia "antigo → novo" com o problema de cada linha, rodapé dizendo o que o Enter fará.
- **Dependência:** `regex = "=1.13.2"` no `fx-core`. Não é crate novo: a mesma versão já vinha no `Cargo.lock` (dependência do GPUI). Limite de tamanho da regex compilada: 1 MB.

## D-044 — Prévias completas e o processo `fx-preview` (T2.9)
- **Data:** 2026-11-22 · **Status:** aceita
- **Visualizadores do Windows (PDF, Office, mídia, SVG, RTF...):** quando o tipo não tem prévia própria do fx e há um preview handler registrado (`AssocQueryStringW` com `{8895b1c6-…}`), o Quick Look inicia `fx-preview.exe`, que cria uma janela sem borda sobre o corpo do painel e hospeda nela o `IPreviewHandler` (inicializado por stream, caminho ou item do shell; normalmente o Windows o roda no `prevhost.exe`). Protocolo pelas entradas e saídas padrão: o host diz `ready` (ou `error …`), o fx manda `move x y w h`, `raise`, `hide`, `quit` (`fx-shell::preview`, `fx-shell::windows::preview_host`).
- **Isolamento e tempo limite:** um visualizador é código de terceiros; travado ou caindo, só o `fx-preview` sofre. O fx espera o `ready` numa thread com limite de 5 s e mata o processo se passar (teste com um host falso que trava: o erro volta no tempo limite). Trocar de item, fechar o painel ou sair encerra o processo.
- **Armadilhas encontradas na conferência:**
  - **Dono da janela:** a janela do host **não** pode ser filha nem ter como dona a janela do fx. Uma relação de janelas entre processos liga as filas de entrada, e o teclado do fx parou. Por isso ela é solta: o fx a mantém acima de si com `raise` a cada 250 ms enquanto está ativo (um clique no fx o traria para cima dela) e manda `hide` quando perde o foco.
  - **Console:** o `fx-preview` é um programa gráfico (`windows_subsystem = "windows"`) e é iniciado com `CREATE_NO_WINDOW`; como programa de console, o Windows abria um terminal que roubava o foco.
  - **Posição:** um `canvas` absoluto precisa de `top_0()/left_0()`, senão o layout o coloca depois do irmão; isso deslocava o host uma altura inteira (corrigido também no `canvas` das migalhas).
- **Repintura:** enquanto o host abre, o fx pede um quadro por repaint; depois que ele aparece, um temporizador de 250 ms repinta só o necessário para mantê-lo no lugar (sem redesenhar a 239 Hz).
- **No fx, sem dependências novas** (o plano previa `zip`, `pulldown-cmark` e `tree-sitter`):
  - **ZIP:** lê só o diretório central (sem descompactar), com ZIP64, nomes UTF-8 e CP437, até 5.000 entradas (`fx-core::zip_list`);
  - **Markdown:** títulos, listas (inclusive tarefas), citações, código e réguas, com os marcadores em linha reduzidos ao texto (`fx-core::markdown`);
  - **Código:** tokenizador por família (Rust; C/C++/C#/Java/JS/TS/Go/Kotlin/Swift/CSS/PHP; Python; shell e PowerShell; JSON/TOML/YAML/INI; SQL), com palavras-chave na cor de destaque e cores próprias para textos e números (`syntax_string`, `syntax_number` no tema, contraste ≥ 4,5:1 testado) (`fx-core::highlight`).

  O `tree-sitter` custaria vários MB e dezenas de gramáticas para uma prévia de leitura.
- **Nesta máquina:** há visualizador para PDF (Edge), RTF, SVG e texto/HTML; não há para DOCX (exige Office) nem para MP4/MP3 (o Windows 11 não traz o de mídia). Esses tipos mostram a miniatura do Windows (o vídeo, um quadro).
- **Limites:** mover a janela do fx só reposiciona o host na repintura seguinte (até 250 ms); um menu ou a paleta abertos sobre o Quick Look ficam atrás do host.

## D-045 — Instalador (T2.10)
- **Data:** 2026-11-22 · **Status:** aceita
- **Ferramenta:** Inno Setup 6, no CI (o runner do GitHub tem ou instala pelo Chocolatey); nada é instalado na máquina de desenvolvimento. Escolhido por ser gratuito, maduro e gerar um `.exe` único com desinstalador; MSI (WiX) fica para a Store/empresas se for pedido.
- **Por usuário:** `PrivilegesRequired=lowest`, pasta `%LOCALAPPDATA%\Programs\fx`, atalho no Menu Iniciar (área de trabalho opcional), português e inglês. Instala `fx.exe`, `fx-indexd.exe`, `fx-preview.exe`, licenças, README e CHANGELOG.
- **Desinstalar:** remove tudo que instalou; pergunta (padrão "Não") se apaga também `%APPDATA%\fx` e `%LOCALAPPDATA%\fx`; se o serviço de índice existir (`HKLM\...\Services\fx-indexd`), roda `fx-indexd uninstall` com UAC, que apaga o serviço e sua cópia em Program Files (D-040).
- **CI:** o job `check` compila o instalador a cada push (artefato `fx-setup`, falha se o script não compilar); a tag anexa `fx-X.Y.Z-setup-x64.exe` ao release ao lado do zip portátil.
- **Pendente:** o teste num Windows limpo ficou para a rodada final (`docs/TESTES-FINAIS.md`), a pedido do usuário.

## D-046 — fx como gerenciador padrão, com reverter (T3.1)
- **Data:** 2026-11-22 · **Status:** aceita (código e testes; aplicar no PC do usuário só quando ele pedir, SAFETY.md §1)
- **Onde:** só `HKCU\Software\Classes` (sem administrador, só o usuário atual): `Folder`, `Directory` e `Drive` → `shell\open\command` = `"fx.exe" "%1"`, e `CLSID\{52205fd8-5dfb-447d-801a-d0b52f2e83e1}\shell\opennewwindow\command` = `"fx.exe"` (o verbo que o Win+E executa). Em todas, `DelegateExecute` vazio, para o Explorador usar o comando em vez do seu manipulador COM. Mesma técnica usada por outros gerenciadores (Files, Directory Opus).
- **Reverter:** antes da primeira ativação, o fx guarda em `HKCU\Software\fx\DefaultManager` os valores que vai substituir (ou "ausente") e, por chave, o primeiro componente do caminho que não existia. Reverter restaura os valores, apaga só essas chaves criadas e por fim a cópia. Ativar de novo mantém a primeira cópia, então o reverter sempre volta ao estado de antes do fx. Um item de outro programa sob as mesmas chaves não é tocado (teste).
- **Testes:** todas as funções recebem a raiz; os testes usam `HKCU\Software\fx-test-<pid>-<nome>` e apagam a raiz no fim (confirmado: nenhuma chave de teste sobra). Nenhum teste toca as chaves reais.
- **Sem argumento:** o fx agora abre a pasta do usuário (`USERPROFILE`) quando não recebe pasta (Win+E, Menu Iniciar); antes abria a pasta de trabalho, que nesses casos é a System32. Argumento vazio conta como ausente.
- **No app:** paleta → "Tornar o fx o gerenciador de arquivos padrão (pastas e Win+E)" e "Voltar ao Explorador como gerenciador padrão", numa thread de lançamento, com aviso do resultado.

## D-047 — Painel duplo (T3.2)
- **Data:** 2026-11-22 · **Status:** aceita
- **Modelo:** o segundo painel é outro `FolderView` guardado pelo `TabsView` (`tabs_view/dual.rs`), indexado pela vista principal da aba, sem mudar a estrutura das abas. `TabsView::current()` devolve o painel com foco; a paleta, a lateral, a acessibilidade e "nova aba" passaram a usá-lo.
- **Teclas:** F3 abre/fecha (abre na mesma pasta, já com o foco); Tab troca de painel, a menos que o painel focado use o Tab (campo de caminho, busca, renomear em lote: `FolderView::uses_tab`); F5 copia e F6 move a seleção (ou o item sob o cursor) para a pasta do outro painel, com `Job::copy`/`Job::move_to` pela fila do painel de origem (conflitos, histórico, Ctrl+Z). Clicar num painel o ativa (o foco do GPUI é lido a cada render). Uma faixa de 2 px na cor de destaque marca o painel ativo.
- **Fechar a aba** descarta o segundo painel.
- **Pendente:** salvar o painel duplo na sessão (por aba); arrastar entre os painéis já funciona pelo arrastar e soltar normal.

## D-048 — Vista em colunas (T3.3)
- **Data:** 2026-11-22 · **Status:** aceita
- **Modelo** (`fx-core/src/miller.rs`): colunas a partir do caminho (as 3 últimas pastas), cada uma com o item escolhido; escolher uma pasta abre a coluna seguinte e fecha as que vinham depois; ← na primeira coluna acrescenta a pasta-mãe (continua subindo); → entra na coluna da pasta escolhida.
- **Interface** (`folder_view/columns.rs`): camada sobre a área da lista (abaixo da barra de caminho), colunas de 260 px numa linha com rolagem horizontal que acompanha a coluna ativa; cada coluna lida por `fx_fs::enumerate` numa thread (`fx-column`) e desenhada com `uniform_list`, pastas primeiro em ordem natural, ocultos conforme Ctrl+H. Clique escolhe; duplo clique abre arquivo. Esc ou Ctrl+Shift+3 fecham e levam a lista para a pasta da coluna ativa.
- **Atalho:** Ctrl+Shift+3, lido pelo estado do teclado como Ctrl+Shift+2/6 (D-034), por causa do Shift+dígito do GPUI.
- **Pendente:** arrastar, menu de contexto e Quick Look dentro das colunas; salvar o modo na sessão.

## D-049 — Etiquetas coloridas (T3.4)
- **Data:** 2026-11-22 · **Status:** aceita
- **Modelo:** até 7 por item numa máscara de bits (`fx-core::tags`: vermelho, laranja, amarelo, verde, azul, roxo, cinza; nomes em português e inglês ou 1–7).
- **Onde ficam:** `%LOCALAPPDATA%\fx\tags.db` (SQLite, já no projeto), tabela `tags(key, path, mask)`, com a chave em minúsculas (o NTFS ignora maiúsculas) e o caminho como escrito, para exibir. Um espelho em memória atende a interface (um lock e uma busca por hash); carregar e gravar acontecem na thread `fx-tags`. Com `FX_NO_STATE=1`, só em memória.
- **Acompanham o fx:** ao terminar uma operação, o que o relatório diz que foi renomeado ou movido (inclusive em lote) leva as etiquetas; uma pasta leva as de tudo que está dentro (sem confundir `Nova` com `Nova2`). Mudanças feitas fora do fx não são vistas (o item etiquetado que sumiu simplesmente não aparece na busca).
- **Teclado:** Alt+1…7 liga ou desliga a etiqueta na seleção (desliga em todos quando todos já têm), Alt+0 limpa. Na lista, um ponto de 8 px por etiqueta ao lado do nome.
- **Busca:** `etiqueta:verde` (ou `tag:`) responde direto das etiquetas, em qualquer pasta, e aplica depois o nome e os outros filtros.
- **Pendente:** etiquetas na grade e nas colunas; editar os nomes das etiquetas; filtro por etiqueta na paleta.

## D-050 — Terminal embutido (T3.5)
- **Data:** 2026-11-22 · **Status:** aceita
- **ConPTY** (`fx-shell/src/windows/conpty.rs`): `CreatePseudoConsole` sobre dois pipes, o shell iniciado com o atributo `PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE`. Armadilha: com as saídas do processo pai redirecionadas (testes pelo WSL), o filho herdava esses handles em vez do console; `STARTF_USESTDHANDLES` com handles nulos resolve. Fechar o painel fecha o console e encerra o processo.
- **Tela** (`fx-core/src/vt.rs`, sem dependências): UTF-8 (inclusive caractere dividido entre leituras), CR/LF/BS/TAB, CSI de cursor (A B C D G H f), apagar (J K X), SGR (16 cores, negrito; 256 cores e RGB são pulados), OSC ignorado, quebra de linha automática, 2.000 linhas de histórico. O que não conhece é ignorado.
- **Painel** (`folder_view/terminal.rs`): Ctrl+J abre o PowerShell na pasta atual (280 px, embaixo da aba, sobre a lista) e fecha; enquanto tem o foco recebe todas as teclas (Esc inclusive), codificadas em VT (setas, Home/End, Delete, PgUp/PgDn, Ctrl+letra); clicar na lista devolve o foco a ela. Cores Campbell (as do Windows Terminal). Um `canvas` mede o painel e o console é redimensionado. A saída repinta a vista em rajadas agrupadas.
- **Pendente:** seguir a pasta da aba (`cd`), rolar o histórico com a roda, copiar texto, escolher o shell.

## D-051 — Alto contraste e DPI por monitor (T3.8)
- **Data:** 2026-11-22 · **Status:** aceita
- **Detecção:** `SystemParametersInfoW(SPI_GETHIGHCONTRAST)`; as cores vêm de `GetSysColor` (janela, texto, destaque, texto cinza, link, botão), expostas por `fx_shell::high_contrast()` com o tipo neutro `SystemColors`.
- **Tema:** `Theme::from_system` usa só essas cores, opacas: fundo e camada = janela, texto e texto secundário = texto da janela (alto contraste evita cinzas em texto), destaque e foco = cor de seleção do sistema, divisórias na cor do texto. Claro ou escuro pela luminância do fundo. O Mica fica desligado. `Theme::current` prefere o alto contraste ao modo claro/escuro em todos os lugares onde o tema é escolhido.
- **Ao vivo:** o GPUI só avisa quando muda claro/escuro (`ImmersiveColorSet`), e ligar o alto contraste nem sempre muda isso; uma tarefa consulta o estado a cada 1 s (uma chamada de sistema barata, fora de qualquer espera) e reaplica o tema quando ele muda. Evita mexer no patch do GPUI.
- **DPI por monitor:** já atendido: o manifesto do GPUI declara `PerMonitorV2` e o GPUI trata `WM_DPICHANGED` (redesenha na escala nova). Nada a fazer no fx além de não usar tamanhos em pixels físicos.
- **Pendente:** cores fixas que não passam pelo tema (terminal embutido, etiquetas, realce de sintaxe em parte) seguem iguais no alto contraste; conferência com o Narrador e com o alto contraste ligado vai para os testes finais.

## D-052 — Build ARM64 (T3.6)
- **Data:** 2026-11-22 · **Status:** aceita
- **Como:** compilação cruzada no próprio runner `windows-latest` (x64), com o alvo `aarch64-pc-windows-msvc` do `rust-toolchain.toml` e as ferramentas ARM64 do MSVC que o runner já traz. Job separado (`arm64`), em paralelo ao `check`; o `release` depende dos dois.
- **Instalador:** o mesmo `installer/fx.iss` com `/DArch=arm64` (`ArchitecturesAllowed=arm64`, nome `fx-X.Y.Z-setup-arm64.exe`). O executor do Inno Setup é x86 (roda por emulação no Windows ARM); os binários instalados são ARM64.
- **Release:** a tag gera também `fx-X.Y.Z-windows-arm64.zip` e anexa o instalador ARM64.
- **Local:** `cargo xwin` no WSL não compila ARM64 hoje (o `ring`, dependência do GPUI, precisa dos cabeçalhos ARM64 que o xwin não baixou); o CI é a referência. Sem teste em máquina ARM real: vai para os testes finais.

## D-053 — Português e inglês (T3.7)
- **Data:** 2026-11-22 · **Status:** aceita
- **Catálogo no ponto de uso, não num arquivo central:** cada texto é escrito como par, `tr!("Copiar", "Copy")` (texto fixo) ou `trf!("Moveu {n} itens", "Moved {n} items")` (mesmas regras do `format!`, inclusive capturas). As macros ficam em `fx-core::locale`, com a chave global do idioma; `fx-ui/src/i18n.rs` escolhe o idioma e guarda o teste. Motivo: com dois idiomas, o par junto do uso é mais fácil de manter que chaves num arquivo separado, e o compilador garante as duas versões (o critério "todo texto tem as duas versões" vira erro de compilação). Desvio consciente do texto da tarefa, que falava em catálogo em `i18n.rs`.
- **Comandos da paleta:** a tabela é `const`, então guarda o par `(pt, en)` e `Command::label()` escolhe na hora.
- **Escolha do idioma:** `idioma` em `settings.toml` ("pt"/"en"), depois `FX_LANG`, depois `GetUserDefaultUILanguage` (português de qualquer país → português; o resto → inglês). A paleta troca ao vivo (`refresh_windows`) e a escolha é salva com a sessão, sem I/O na thread de UI.
- **Formatos:** separador de milhar `.`/`,`, decimal `,`/`.`, data `DD/MM/AAAA`/`MM/DD/AAAA` (24 h nos dois), "itens"/"items", tipo "Arquivo JPG"/"JPG File"; rótulos dos filtros de busca e problemas do renomear em lote também. As palavras-chave da busca já aceitavam os dois idiomas (`tipo:`/`type:`).
- **Valores guardados não são traduzidos:** a densidade em `settings.toml` continua "padrão"/"compacta"/"confortável".
- **Teste de varredura:** lê `fx-ui/src`, ignora comentários, SQL e o módulo de testes, e acusa todo literal com cara de português (acentos ou palavras como "de", "para", "não") que não seja a primeira metade de um par. Achou dois esquecidos na primeira rodada.
- **Pendente:** mensagens de erro vindas de `fx-ops` e `fx-shell` (ex.: "já existe um item chamado …") seguem em português; as do próprio Windows vêm no idioma do sistema.

## D-054 — Pastas de nuvem (T3.9)
- **Data:** 2026-11-22 · **Status:** aceita
- **Estado só pelos atributos** (o que a enumeração já traz, sem custo): `PINNED` → sempre neste dispositivo; senão `RECALL_ON_DATA_ACCESS`/`RECALL_ON_OPEN`/`OFFLINE` → só na nuvem; senão `UNPINNED` → disponível; senão nenhum. Vale para o OneDrive e para qualquer provedor da API de arquivos de nuvem do Windows. `Attributes` passou de 8 para 16 bits.
- **Nunca ler o conteúdo de um item só na nuvem pela lista:** a grade não pede miniatura e a lista/grade não pede ícone por arquivo (exe, lnk, ico) para ele (`icon_key_cloud` cai para o ícone da extensão). Pré-visualizar (Espaço) e abrir continuam baixando, porque o usuário pediu; abrir já roda fora da thread de UI.
- **Ícones:** Segoe Fluent `E753` (nuvem, cor secundária), `E930` (visto vazado) e `EC61` (visto cheio), estes na cor de destaque, ao lado do nome, como as etiquetas.
- **Comandos:** "Manter sempre neste dispositivo" e "Liberar espaço" na paleta mudam só os atributos `PINNED`/`UNPINNED` (é o que `attrib +P -U` / `-P +U` fazem e o que o Explorador usa); o provedor baixa ou libera. Recursivo em pastas, em thread de fundo, com aviso no fim. O menu de contexto do shell continua mostrando os itens do próprio provedor.
- **Pendente:** placeholders de nuvem também são pontos de nova análise, marcados `LINK`; a busca por pasta não desce em subpastas de nuvem (ajustar com a etiqueta de reparse). Conferência com um OneDrive de verdade vai para os testes finais.

## D-055 — Relatório de falhas (T3.10)
- **Data:** 2026-11-22 · **Status:** aceita
- **Só local, nada enviado pelo fx:** o hook de pânico grava `%LOCALAPPDATA%\fx\falhas\falha-<unix>-<pid>.txt`. Na abertura seguinte o status diz "Falha na última abertura · Ctrl+K: relatar"; o comando "Relatar a última falha" abre no navegador `github.com/heapwraith/fx/issues/new` com título e corpo preenchidos (até 6.000 caracteres). O usuário lê e decide enviar. O relatório é então renomeado para `.visto` (não é oferecido de novo, fica no disco).
- **Sem dados pessoais** (`fx-core/src/crash.rs`, puro e testado): versão com o commit, módulo e `crate/src/arquivo.rs:linha` (sem as pastas acima do crate), mensagem e só os nomes das funções da pilha (as linhas `at caminho:linha` saem; a maquinaria de pânico também). Na mensagem e na pilha: caminhos (`C:\…`, `\\…`, `/a/b`) viram `<caminho>`, texto entre aspas (nomes de arquivo) vira `<texto>`, e o `USERNAME` vira `<usuário>` onde aparecer, sem diferenciar maiúsculas.
- **Desligar:** `relatorio_falhas = false` no `settings.toml`, ou "Relatórios de falha: ligar ou desligar" na paleta (salvo com a sessão). Desligado, nada é gravado (nem a pasta). Padrão ligado, porque o relatório não sai do computador sem ação do usuário.
- **Substitui** o log cru em `%TEMP%\fx-crash-<pid>.log` (registrado na Fase 1), que tinha caminhos completos.
- **Teste manual:** `FX_TEST_PANIC=1` faz o fx entrar em pânico logo ao abrir (só para conferir o relatório). Desde a D-066, só em build de depuração.

## D-056 — Atualizador (T4.1)
- **Data:** 2026-11-22 · **Status:** aceita
- **Sem crates novas:** HTTPS pelo WinHTTP (proxy e certificados do sistema, só `https://`, redirecionamentos do GitHub para o armazenamento de arquivos seguidos pelo próprio WinHTTP, limite de tamanho por resposta), SHA-256 pelo BCrypt, assinatura pelo WinVerifyTrust (`fx-shell/src/windows/net.rs`). O JSON dos releases é lido por um leitor mínimo em `fx-core::update::json` (objetos, listas, textos com escapes e pares substitutos, limite de profundidade), testado.
- **Quando:** uma vez por dia (`update-check.txt` ao lado do `state.db`), na primeira janela, numa thread de fundo; avisa só se houver versão nova ("Versão X disponível · Ctrl+K: atualizar"). "Atualizar o fx" na paleta procura na hora se ainda não souber de nenhuma. `verificar_atualizacoes = false` (ou a paleta) desliga a consulta diária; benchmarks e testes (`FX_NO_STATE=1`) nunca acessam a rede. A consulta não manda dados do usuário (User-Agent `fx-updater`; só o IP chega ao GitHub).
- **Qual versão:** `GET /repos/heapwraith/fx/releases`, ignorando rascunhos; a maior versão semver acima da atual (pré-versões contam: o projeto está em 0.x) que tenha o `fx-X-setup-<arq>.exe` e o `SHA256SUMS.txt`.
- **Instalar:** baixa o instalador para `%TEMP%\fx-update`, confere o SHA-256 contra o `SHA256SUMS.txt` do mesmo release (o CI gera, junto dos zips e instaladores). Se o fx em uso for assinado (depois da SignPath), exige o instalador assinado. Roda `/SILENT /SUPPRESSMSGBOXES /NORESTART /CLOSEAPPLICATIONS /NORESTARTAPPLICATIONS /RELAUNCH`: o Restart Manager fecha o fx (pelo `WM_CLOSE`, que salva a sessão) e uma entrada nova do `[Run]` do `fx.iss`, só com `/RELAUNCH`, abre o fx novo.
- **Delta descartado:** o instalador tem ~6 MB; pacotes delta trariam um formato próprio, assinatura e casos de falha a mais para economizar poucos MB por versão.
- **Limite conhecido:** sem assinatura ainda, a integridade depende do HTTPS do GitHub e do `SHA256SUMS.txt` no mesmo release (protege contra download corrompido, não contra quem controlar o release). A assinatura fecha isso.

## D-057 — Polimento final (T4.2)
- **Data:** 2026-11-22 · **Status:** aceita
- **Transições:** só na entrada das sobreposições (paleta, menu Exibir, busca, Quick Look, renomear em lote, histórico, diálogo de conflito), 120 ms com `ease_out_quint`, opacidade de 0,35 a 1 (`fx-ui/src/motion.rs`). A sobreposição existe e recebe teclas desde o primeiro quadro; o estado da animação fica preso ao id do elemento, então digitar não a reinicia. Saída sem animação (fechar é instantâneo). Com "Efeitos de animação" desligado no Windows (`SPI_GETCLIENTAREAANIMATION`, lido ao abrir) nada anima. Teste: duração ≤ 150 ms e nenhuma com a opção desligada.
- **Estados vazios:** busca sem resultado e paleta sem resultado mostram uma frase onde estariam os resultados, com o próximo passo ("Tab busca no disco inteiro" só quando há busca no disco; senão "Tente menos letras ou outro filtro"). Pasta vazia, só ocultos, filtro sem resultado e erros de leitura já tinham frase (Fase 1).
- **Erros nos dois idiomas:** as mensagens de `fx-ops` (Lixeira, confirmação de operações perigosas, nomes, cópia) e de `fx-shell` (menu, Propriedades, prévia, elevação) viraram pares `tr!`/`trf!`; o `fx-preview` recebe o idioma do fx por `FX_LANG`. O relatório de falhas (D-055) passa a usar rótulos neutros em inglês (`module`, `location`, `<path>`, `<user>`, `<text>`), por ir para uma issue pública.
- **Varredura de textos** (T3.7) agora cobre `fx-core`, `fx-ops`, `fx-shell` e `fx-fs`, entende strings cruas (`r"…"`, `r#"…"#`), corta o módulo de testes em qualquer `#[cfg(...test...)]`, ignora `tests.rs`, tabelas de caracteres e palavras soltas sem acento (palavras-chave de linguagens como "as", "do").
- **Medido:** P4 p95 3,41 (paleta), 3,34 (setas), 3,45 (busca); Quick Look p95 50 ms; P8 em três rodadas 87,6, 86,8 e 86,0 MB (portão 3: 86,9), sem regressão.
- **Não feito:** passada nas três densidades e em janela estreita (as telas novas usam os mesmos componentes); transição de saída.

## D-058 — Instalador próprio, fx-setup (T4.3)
- **Data:** 2026-11-22 · **Status:** aceita (escolha do usuário: "a mais coerente, não a mais fácil"; inspiração: Zed)
- **Por quê:** o instalador é a primeira tela do fx; um instalador genérico não combina com um app nativo e cuidado. Com GPUI, a mesma interface (tokens do `theme.rs`, incluído por `#[path]`, sem duplicar) roda nos três sistemas quando o porte vier; as ações de instalação ficam em `fx-shell` (D-012), só Windows hoje. No Mac e no Linux o plano é seguir o formato nativo (`.dmg` de arrastar, Flatpak/AppImage), compartilhando o atualizador.
- **Pacote** (`fx-core::package`): o `fx-setup.exe` é o programa seguido do pacote e de um rodapé (`FXSETUP1`, deslocamento e tamanho). O pacote tem a versão, a lista de arquivos (nome simples, tamanho, SHA-256) e todos os arquivos comprimidos juntos com LZMS (Compression API do Windows, sem crates novas). Ler confere tudo: nomes perigosos, tamanhos, hashes, bytes sobrando. O próprio programa do instalador vai dentro, e é instalado como desinstalador.
- **Instalar** (`fx-setup/src/transaction.rs`): por usuário em `%LOCALAPPDATA%\Programs\fx` (mesmo lugar do Inno, então adota a instalação antiga e remove a chave dela). Escreve tudo numa pasta temporária ao lado, depois troca arquivo por arquivo: o atual é renomeado para `*.fx-old-<pid>` (funciona até com o fx aberto, que o Windows deixa renomear mas não apagar) e o novo entra; qualquer falha devolve o que havia. Depois: atalho no Menu Iniciar (e na área de trabalho, se pedido) e a entrada em `HKCU\…\Uninstall\fx` (nome, versão, ícone, tamanho, desinstalar e desinstalar em silêncio). Os arquivos postos de lado são apagados quando possível.
- **Desinstalar:** o `fx-setup.exe` instalado se copia para `%TEMP%` e roda de lá (não dá para apagar o próprio executável). Recusa uma pasta que não seja de instalação do fx e, se o fx estiver aberto, não apaga nada ("feche todas as janelas do fx"). Apaga só os arquivos que instalou (nunca a pasta com conteúdo desconhecido), atalhos e registro; as configurações só se a pessoa marcar. Avisa se o serviço `fx-indexd` continua instalado.
- **Telas:** boas-vindas com o slogan ("Pastas abrem na hora. Arquivos aparecem enquanto você digita."), "Opções" discretas (atalho na área de trabalho, pasta), progresso que desliza, "Tudo pronto" com dica de Ctrl+K e F1, erro com "Tentar de novo", confirmação e fim da desinstalação. Enter e Esc. Atualizar ou reinstalar diz de qual versão para qual.
- **Linha de comando:** `--silent` (atualizador e CI), `--uninstall [--quiet]`, `--pack <programa> <pasta dos binários> <pasta dos documentos> <saída>` (CI; o x64 empacota o programa ARM64). `--test-root <pasta>` manda pasta, atalhos e registro para um lugar de teste (testes de ponta a ponta e CI; era a variável `FX_SETUP_TEST_ROOT` até a D-066).
- **CI:** gera `fx-X-setup-x64.exe` e `-arm64.exe` com `--pack` e instala o x64 em silêncio numa pasta de teste. O Inno Setup e `installer/fx.iss` saíram.
- **Atualizador (T4.1):** passa a rodar `fx-setup --silent`; a versão nova vale na próxima abertura do fx (a T4.4 faz a troca ao fechar, como o Zed).
- **Custo:** 15,2 MB contra 5,8 MB do Inno, porque o programa do instalador leva o GPUI (7 MB, sem compressão). Pendências: a cópia em `%TEMP%` usada para desinstalar fica lá; com o `fx-indexd` instalado, a cópia em Program Files não é atualizada.

## D-059 — "Sobre", "Como usar" e o slogan (T4.6)
- **Data:** 2026-11-22 · **Status:** aceita (pedido do usuário)
- **Slogan:** "Pastas abrem na hora. Arquivos aparecem enquanto você digita." / "Folders open instantly. Files appear as you type." Vem das duas promessas medidas do projeto (P2 e P5), sem adjetivo vazio. Usado no instalador e no "Sobre".
- **"Sobre o fx"** (paleta): logo, nome, slogan, versão e compilação, licença, "Feito em Rust, nativo do Windows" (sem citar o Zed na interface, a pedido do usuário; a atribuição do GPUI fica na licença, ver abaixo), e links: Procurar atualizações, Novidades (CHANGELOG), Código e sugestões (GitHub), Como usar. Substitui o aviso no status.
- **"Como usar o fx"** (F1, paleta e sozinha na primeira abertura, `guia_visto` no `settings.toml`): seis grupos (Navegar, Buscar, Arquivos, Ver, Abas e painéis, Organizar) com os atalhos que existem no código, em duas colunas, rolável em janelas baixas. Esc ou clique fora fecha.
- **Textos do instalador** reescritos: título que acolhe ("Bem-vindo ao fx"), o slogan como subtítulo, uma frase prática ("Leva poucos segundos. Instala só para você, sem pedir senha de administrador."), erros que dizem o que fazer, e a versão discreta no rodapé.

## D-060 — Atualização ao fechar, como o Zed (T4.4)
- **Data:** 2026-11-22 · **Status:** aceita (substitui a parte "instalar" da D-056)
- **Fluxo:** a consulta diária (ou "Atualizar o fx") acha a versão nova, baixa o `fx-setup` dela, confere o SHA-256 (e a assinatura, se o fx em uso for assinado) e o guarda em `%LOCALAPPDATA%\fx\update` com um `ready.txt` (versão e nome). O status diz "Versão X pronta · entra quando você fechar o fx (Ctrl+K: reiniciar agora)".
- **Aplicar:** no `on_app_quit` do GPUI (fechar a última janela também encerra o app), o fx inicia `fx-setup --silent --wait <pid>`: o instalador espera o fx terminar (até 60 s) e troca os arquivos com a transação da T4.3. "Atualizar o fx" com uma versão pronta salva a sessão na hora (`Persist::flush`, num `Global`), inicia o instalador com `--relaunch` e encerra; o fx novo abre sozinho. O `ready.txt` sai ao aplicar; o instalador baixado é apagado na abertura seguinte, quando a versão em uso já não é menor que ele.
- **Desligar:** `verificar_atualizacoes = false` desliga a consulta e o download automático; "Atualizar o fx" na paleta continua funcionando quando pedido. Muda a regra inicial da Fase 4 ("nada é baixado sem pedir") para o modelo do Zed, com a opção.
- **Achado:** salvar a sessão de dentro da atualização da aba (`FolderView`) fazia o GPUI entrar em pânico ("cannot read FolderView while it is already being updated"): o salvamento lê todas as abas. Agora acontece no nível do app (`AsyncApp::update`). Os relatórios de falha passaram a ter um contador no nome: dois pânicos no mesmo segundo sobrescreviam o primeiro, que era o útil.
- **Teste de ponta a ponta** (manual, pasta e chave de teste): `--pack … --version <v>` gera pacotes com versão maior (era `FX_PACK_VERSION` até a D-066); 0.2.0 → 9.9.9 ao fechar a janela, 9.9.9 → 9.9.10 pela paleta, com o fx reaberto.

## D-061 — Documentação reorganizada para a Fase 5
- **Data:** 2026-11-22 · **Status:** aceita (pedido do usuário: "mantenha apenas o necessário")
- `docs/ROADMAP.md` substitui `docs/phases/PHASES-1-4.md` (que descrevia uma Fase 4 diferente da que foi feita): as seis fases, o estado, o que cada uma entregou e os links dos portões.
- README reescrito para quem usa (o que é, números, recursos, instalar, primeiros passos, privacidade) e para quem contribui (build e mapa dos documentos).
- `ARCHITECTURE.md` atualizado com os 11 crates e os 4 processos reais; `PRODUCT.md`, `PERFORMANCE.md`, `SAFETY.md` (seção do instalador), `SIGNING.md`, `TESTES-FINAIS.md` e `CHANGELOG.md` atualizados.
- Removidas as capturas da T1.1 (`bench/results/visual-t1.1/`), já aprovadas e sem uso.
- Os arquivos de fase e os relatórios de portão ficam como histórico.

## D-062 — Bateria final de testes e correções
- **Data:** 2026-11-22 · **Status:** aceita
- **Como:** 327 testes automáticos e uma passada pelo fx de verdade, numa pasta descartável, em português e inglês, tema escuro e claro: lista, grade, colunas, painel duplo, filtro, busca com filtros, Quick Look de cada tipo, etiquetas, terminal, histórico, menu Exibir, caminho com sugestões, renomear em lote, paleta, cópia grande, conflito, desfazer, nova pasta, renomear, excluir, abas, "Sobre" e "Como usar".
- **Regra nova do usuário:** nenhum status pode ser simulado; todo progresso vem de trabalho medido (memória do agente e `fx-design`).
- **Corrigido:**
  - **Diálogo de conflito não capturava o teclado:** Enter e Ctrl+Z iam para a lista atrás (o Ctrl+Z chegou a desfazer a cópia com o diálogo aberto). Agora é modal: Enter mantém os dois, Esc pula, Espaço marca "aplicar a todos"; a dica aparece no diálogo.
  - **Confirmações eram a caixa nativa do Windows** (título "Critical" em inglês): renderizador próprio de prompts do GPUI (`prompt.rs`), com os tokens do tema, nos dois idiomas, botão vermelho para excluir de vez, Enter confirma, Esc cancela.
  - **Delete mandava para a Lixeira sem perguntar** (como o Explorador desde o Windows 8): agora pergunta; "Confirmar antes de enviar para a Lixeira" na paleta (ou `confirmar_lixeira = false`) desliga.
  - **Barra de progresso quase invisível:** 6 px com trilho visível, mais bytes, porcentagem, velocidade e tempo restante calculados dos bytes copiados; uma operação concluída fica 1,5 s como "Concluído" (resultado real), para que cópias rápidas também sejam vistas.
  - **O temporário da cópia aparecia na lista** (`.nome.fx-<pid>-<n>.tmp`): tratado como oculto (`fx_core::list::is_fx_temp`).
  - **"Sobre" e "Como usar" difíceis de achar:** botão de ajuda (?) ao lado do histórico, com os dois.
  - **Instalador com progresso simulado** (animação entre etapas, 2% inicial falso, 88% e 95% fixos, etapa inventada na desinstalação): a barra agora é a fração de trabalho feito (cada arquivo uma unidade, mais atalhos e registro), e a desinstalação reporta cada arquivo removido.
  - **Vista em colunas cortava a coluna atual:** desenha só as colunas que cabem, sempre com a em foco, preferindo as da direita.
  - **A lista aparecia pelas bordas** do Quick Look, da busca e do renomear em lote: véu suave atrás (`theme::scrim`; nenhum no alto contraste).
  - "10000 selecionados" sem separador de milhar; rótulos da paleta cortados em inglês (encurtados nos dois idiomas); `{nome}` no renomear em lote em inglês (agora `{name}`; os dois funcionam); textos cortados no "Como usar".

## D-063 — Configurações e rodapé da barra lateral (padrão global de 2026)
- **Data:** 2026-11-22 · **Status:** aceita (pedido do usuário: "padrão global, não padrão Windows")
- **Padrão seguido:** o dos apps de referência de 2026 (Linear, Notion, Slack, Discord, Arc, VS Code, Zed): rodapé fixo na barra lateral com **Ajuda** e **Configurações**; **Ctrl+,** abre as Configurações (atalho universal); Configurações numa janela com categorias à esquerda; **"Sobre"** é a última categoria.
- **Configurações** (`tabs_view/settings.rs`), cada controle ligado ao estado real: Geral (idioma, confirmar Lixeira, reabrir abas), Aparência (densidade, itens ocultos, barra lateral; o tema segue o Windows), Atualizações (automáticas, versão, procurar agora), Privacidade (relatórios de falha; o que o fx envia), Sistema (gerenciador padrão, busca no disco inteiro), Sobre (logo, slogan, versão, licença, Como usar, Novidades, Código, Relatar um problema).
- **Rodapé da barra lateral:** "Ajuda F1" (abre "Como usar") e "Configurações Ctrl+,", sempre visíveis (fora da rolagem). O botão "?" do topo saiu (duplicava). "Sobre o fx" na paleta abre a seção Sobre; "Configurações" também está na paleta.
- `restaurar_sessao` virou uma preferência alterável na hora (salva com a sessão), como as demais.

## D-064 — Novos limites de velocidade e tamanho
- **Data:** 2026-11-22 · **Status:** aceita (aprovada pelo usuário; substitui as metas de D-013 e D-038)
- **Dois níveis:** *limite* (reprova o portão) e *alvo* (onde queremos chegar). Tabela completa em `docs/PERFORMANCE.md`.
- **Apertadas** (já passam e viram proteção contra regressão): P2 ≤ 80 ms e aba nova ≤ 16 ms; P3 ≤ 50 ms; P4 ≤ 8 ms p95; P5 ≤ 16 ms; P7 ≤ 0,1%; Quick Look ≤ 60 ms; P1 ≤ 450 ms p95 (passa no limite).
- **Novas:** P8b (aba ≤ 3 MB), P8c (janela ≤ 20 MB), P10 (cópia ≥ 90% do Explorador), P11 (sobreposições ≤ 16 ms), P12 (arquivo novo na busca ≤ 300 ms), P13 (instalar ≤ 3 s, atualizar ≤ 2 s), S1–S3 (tamanho dos executáveis e do instalador).
- **Mais duras e hoje reprovadas:** P8 ≤ 85 MB, P9 ≤ 100 MB/milhão, S1 `fx.exe` ≤ 12 MB, S2 instalador ≤ 10 MB.
- **Correção:** a P6 (índice de 2 milhões em ≤ 10 s) foi marcada como aprovada no portão 2 por "proporcionalidade", mas 744 mil arquivos em 7,5 s projetam ~20 s para 2 milhões: **não passa**. Corrigido em `gate-2.md` e `gate-4.md`.
- **Plano para as que reprovam** (Fase 5, T5.7): tirar do GPUI o cliente HTTP que ele traz e o fx não usa (`reqwest`, `rustls`, `ring`; o atualizador usa WinHTTP), comprimir o programa do instalador, paralelizar a leitura da MFT por volume e reduzir alocações do índice, e criar no `fx-bench` as medições que faltam.

## D-065 — Fase 6, "Superioridade"
- **Data:** 2026-11-22 · **Status:** aceita como plano (pedido do usuário; nada implementado ainda)
- **Critério:** funcionalidades que tornem o fx superior, não só igual, priorizando o que nasce das vantagens que só ele reúne: índice da MFT do disco inteiro, diário USN em tempo real, motor de operações seguro com desfazer, interface nativa.
- **Ordem:** (1) mapa do disco, linha do tempo, buscas vivas; (2) arquivos compactados como pastas, painel de detalhes; (3) duplicados, "ir para" que aprende, desfazer entre sessões; (4) busca no conteúdo e demais paridades; (5) opcionais de 2026 (busca por significado com IA local, ações rápidas por tipo). Detalhes em `docs/phases/PHASE-6.md`.
- **Condições:** depois da 1.0; metas de D-064 valem para tudo; itens com risco de memória só entram com a P8 dentro do limite.

## D-066 — Revisão de código completa (2026-11-22)
- **Data:** 2026-11-22 · **Status:** aceita (pedido do usuário: corrigir todos os achados da revisão)
- **Origem:** revisão com a skill de revisão de código mais varredura própria, conferida na documentação oficial (Microsoft Learn, GitHub Docs) e em relatos da comunidade (Files, Directory Opus, node-pty). 32 achados; cada grupo abaixo é um commit "Revisão (n/5)".
- **Dependências aprovadas pelo usuário:** `futures` 0.3.34 (canais assíncronos), `log` 0.4.35 (registro em arquivo) e `thiserror` 2.0.21 (tipos de erro). Já eram compiladas como dependências do GPUI; agora são diretas, na mesma versão do `Cargo.lock`. Saiu a dependência de workspace `fx-preview`, que ninguém usava.
- **1 — Confirmações seguras:** Enter segurado (repetição automática) nunca responde um diálogo, e o Enter só vale 300 ms depois que o diálogo aparece (o Enter da paleta confirmava "Excluir permanentemente" sem ser visto); Esc sempre cancela. Diálogos e todas as sobreposições bloqueiam o mouse (`occlude`): o clique não chega mais à lista de trás. Configurações abertas capturam o teclado (Delete ia para a lista escondida); ↑/↓ trocam de seção; F1 fecha as Configurações antes de abrir o guia. Todo envio para a Lixeira (tecla, paleta, menu de contexto) passa por uma função só, que confirma (o menu de contexto pulava a confirmação). O texto "3 itens itens" foi corrigido. Na vista em colunas, Delete, copiar, recortar e colar agem no item escolhido na coluna em foco, nunca na seleção da lista escondida; as demais teclas da lista ficam bloqueadas (F2 explica como renomear). Escolher o idioma já em uso não troca o idioma.
- **2 — Segurança:** o `ready.txt` do atualizador guarda o SHA-256 do instalador; antes de executar, o fx abre o arquivo travado contra escrita, recalcula o hash e, num fx assinado, exige o mesmo editor (nome do certificado), não só "uma assinatura válida" (a Microsoft trata o signatário como verificação à parte); no download, verifica também a revogação. Um fx rodando como Administrador nunca aplica a atualização (a pasta é gravável por qualquer programa do usuário). O pipe de instância única tem o SID do usuário no nome, DACL só para ele, e o segundo processo confere que o servidor é do mesmo usuário antes de entregar a pasta. O pipe do `fx-indexd` atende até 8 clientes, cada um com sua instância e thread: um cliente parado não trava a busca dos outros; o usuário pode ler e gravar, mas não criar instâncias do servidor (`FILE_APPEND_DATA` é `FILE_CREATE_PIPE_INSTANCE` num pipe), e o cliente pede só `FILE_GENERIC_READ | FILE_WRITE_DATA`. O dono do pipe (`OW`, quem o criou: SYSTEM no serviço, o administrador elevado no `fx-indexd run`) mantém acesso total, porque criar confere o acesso pedido contra a DACL nova (achado no teste: sem isso, "acesso negado"). Ganchos de teste fora dos binários publicados: `FX_TEST_PANIC` só em depuração, `FX_SETUP_TEST_ROOT` virou a opção explícita `--test-root`, `FX_PACK_VERSION` virou `--pack … --version <v>`.
