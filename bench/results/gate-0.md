# Portão da Fase 0 — relatório

**Data:** 2026-11-21 · **Commit de referência:** `main` após a T0.10 · **Build:** release (`lto = "fat"`), compilado no WSL com `cargo xwin`, executado no Windows.

## Máquina

| Item | Valor |
|---|---|
| CPU | AMD Ryzen 7 5700X (8 núcleos, 16 threads) |
| RAM | 32 GB |
| GPU | NVIDIA GeForce RTX 5070 |
| Monitor | 1920×1080 a 239 Hz, escala 100% |
| Disco das fixtures | Kingston NV2 1 TB (NVMe), `%TEMP%` no C: |
| Windows | 11 25H2 (10.0.26200) |

Cache de disco **quente** em todas as medidas (as pastas foram abertas antes). Nenhuma medida "frio real" (pós-reinício) foi feita.

## Tarefas da fase

| Tarefa | Estado |
|---|---|
| T0.1 Workspace e GPUI no Windows | ✅ |
| T0.2 Gerador de fixtures | ✅ |
| T0.3 Enumeração de pastas | ✅ |
| T0.4 Lista, ordenação natural e filtro | ✅ |
| T0.5 Lista virtualizada no GPUI | ✅ (P2 da partida falha; ver abaixo) |
| T0.6 Latência de input | ✅ |
| **T0.7 MFT e índice** | ⏳ **implementada; falta a medição elevada (P6, teste USN)** |
| **T0.8 Busca difusa** | ⏳ **implementada; falta o P5 no índice real do C:** |
| T0.9 Acessibilidade | ✅ (não funciona no GPUI; caminho viável) |
| T0.10 Comparação e relatório | ✅ Explorador medido; Files e Everything não instalados (roteiro em `bench/ROTEIRO-COMPARACAO.md`) |

## Metas × resultados

| Meta | Alvo | Resultado | Passou? |
|---|---|---|---|
| **P1** abertura a frio até a primeira lista | ≤ 150 ms | 386–406 ms (mesma medida da P2 a) | ❌ |
| **P2** primeira pintura, 100 mil arquivos | ≤ 100 ms | (a) desde a partida do processo: **mediana 386–406 ms**, p95 400–439<br>(b) app já aberto, desde o pedido da pasta: **mediana 24–25 ms**, p95 27–33 | (a) ❌ (b) ✅ |
| **P3** lista completa e ordenada, 100 mil | ≤ 300 ms | app aberto: **86–117 ms**; desde a partida: 418–474 ms | ✅ (app aberto) |
| **P4** tecla → quadro | ≤ 16 ms (p95) | **p95 3,4 ms**, mediana 2,7, máx. 3,4 (33 teclas reais) | ✅ |
| **P5** busca por tecla, ~2M arquivos | ≤ 50 ms (p95) | índice **sintético** de 2M: **p95 7,0 ms**, máx. 10,1 (83 teclas). Índice real do C:: **não medido** (exige admin) | ⏳ (sintético ✅) |
| **P6** índice da MFT, ~2M arquivos | ≤ 10 s | **não medido** (exige admin). Montagem em memória (sintético 2M): 0,31 s | ⏳ |
| **P7** rolagem, 100 mil itens | sem quadros > 4,2 ms (p95; um quadro a 239 Hz, escolha do usuário) | arraste + roda até o fim: mediana 4,17, **p95 4,38 ms**, máx. 6,3; **1 quadro perdido em 3.425**; ≤ 26 linhas construídas por quadro | ❌ pela letra; ✅ se a meta for 120 Hz (8,3 ms) |
| **P8** memória parado, 1 janela | ≤ 80 MB | **72,4 MB** privados (52 MB de working set) | ✅ |
| **P9** índice por milhão de arquivos | ≤ 150 MB | sintético: **71 MB/milhão**. Real: não medido | ⏳ (sintético ✅) |
| Outras | — | enumeração `FindFirstFileExW` da `flat-100k`: 1º lote 2,9 ms, total 50 ms; ordenar 100 mil 36 ms; filtrar 1 ms; janela responde (máx. 2,9 ms) carregando 1M arquivos via `\\wsl.localhost` | ✅ |

### Sobre a P7 (por que "falha pela letra")

O GPUI desenha no ritmo do monitor (`DwmFlush`): a 239 Hz, cada quadro dura 4,18 ms. Um quadro perdido aparece como ~8,4 ms. Os intervalos entre 4,2 e 4,4 ms são oscilação do relógio do compositor, não quadros perdidos. Em 3.425 quadros de rolagem (arrastando a barra do início ao fim e de volta, e rolando com a roda até o último item), houve **um** quadro de 6,3 ms e nenhum acima disso. Com a meta de 120 Hz do texto original (8,3 ms), passaria com folga.

### Sobre a P1/P2 (a) (D-013)

Do tempo até o primeiro quadro, ~200 ms são o `D3D11CreateDevice` (driver da NVIDIA carregando em cada processo novo), ~40 ms o DirectWrite, ~30 ms o carregamento do executável e ~35 ms a janela e a swap chain. **Nosso código leva ~25 ms.** Já corrigido no patch do GPUI: o dispositivo duplicado (−200 ms) e a compilação de shaders em runtime (−250 ms); era ~800 ms.

## Comparação com concorrentes

| Medida | fx | Explorador | Files | Everything |
|---|---|---|---|---|
| Abrir a `flat-100k` numa **janela nova** (app/processo novo) | **386–406 ms** (processo novo) | **1.048 ms** mediana, p95 1.129 (pasta pequena: 912 ms; o Explorador gasta ~0,9 s só abrindo a janela) | não instalado | — |
| Abrir a `flat-100k` com o **app já aberto** | **24 ms** mediana, p95 27–33 | **264 ms** mediana, p95 292 (navegação na mesma janela) | não instalado | — |
| Busca por tecla, disco inteiro | p95 7,0 ms (sintético 2M) | — | — | não instalado |

**Método:** fx pelo `FX_TRACE` (do evento até o quadro apresentado). Explorador por UI Automation (`bench/explorer-open.ps1`, `bench/explorer-navigate.ps1`): do comando até um `ListItem` aparecer na árvore UIA, com sondagem a cada ~2 ms mais o custo da chamada (resolução ~5–15 ms). Os métodos não são idênticos. Para fechar com vídeo a 120+ fps, use o roteiro em `bench/ROTEIRO-COMPARACAO.md`.

## Critérios do portão

1. **P2, P4, P5 e P7 passaram?** P4 ✅. P2 ✅ com o app aberto e ❌ desde a partida do processo. P7 ❌ só pela letra (1 quadro perdido em 3.425). P5 ⏳ (sintético ✅, falta o C: real).
2. **P6 e P9 passaram ou têm plano?** P9 ✅ no sintético. P6 ⏳: o código está pronto e falta a medição elevada.
3. **Acessibilidade tem caminho viável?** ✅ Sim: AccessKit com `SubclassingAdapter` (D-008).

## Riscos encontrados

- **Partida do processo (P1/P2 a):** limitada pelo driver da GPU, não pelo GPUI em si. O plano B do portão ("renderizador próprio em D3D11 + DirectWrite") **pagaria o mesmo custo**, porque também cria um dispositivo D3D11. Saídas reais: processo residente, como o Explorador (D-013 a), ou primeira janela sem GPU (D-013 b).
- **Patch próprio do GPUI** (`third_party/gpui`, 3 arquivos mais os shaders pré-compilados): custo de manutenção a cada atualização.
- **Acessibilidade:** depende de uma camada própria (AccessKit) que ainda não existe; listas virtualizadas enormes exigem expor só a janela visível.
- **MFT:** `ntfs-reader` carrega a MFT inteira na memória durante a leitura (~1 KB por registro, ~2,5 GB num disco de 2M arquivos), um pico transitório fora da P9. Medir na execução elevada.
- **Medições pendentes que exigem admin** (P5 real, P6, P9 real, teste USN): rodar antes de decidir.

## Recomendação

**Corrigir antes de passar (pendências curtas), não acionar o plano B.**

1. Rodar as medições elevadas e completar P5, P6 e P9 reais: no WSL, `bench/prepare-elevated.sh`; depois, num PowerShell como Administrador, `cd $env:TEMP\fx-elevated; powershell -ExecutionPolicy Bypass -File .\run-elevated.ps1`.
2. Decidir a D-013: recomendo **(a) processo residente** (o fx fica carregado e "abrir pasta" cria só a janela: ~24 ms medidos, contra 264 ms do Explorador navegando e 1.048 ms abrindo janela), mantendo a P1 como meta de "primeira abertura do dia".
3. Decidir a régua da P7: manter 4,2 ms (o protótipo já fica a 1 quadro perdido em 3.425) ou voltar à meta escrita de 120 Hz.

O GPUI não é o gargalo em nenhuma medida do nosso controle: pintura, rolagem e input ficam dentro de um quadro a 239 Hz. A decisão de passar o portão é do usuário.
