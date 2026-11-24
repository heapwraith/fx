# Portão da Fase 5 — relatório

**Data:** 2026-11-23 · **Versão:** 0.4.0-beta + `main` (commit f1eda89) · **Build:** release, compilado no WSL com `cargo xwin`, executado no Windows.

**Critério do portão:** lançamento da 1.0, com testes manuais feitos, pacote assinado publicado e todas as metas de `docs/PERFORMANCE.md` (D-064) dentro do limite.

## Máquina

A mesma das fases anteriores: AMD Ryzen 7 5700X, 32 GB, RTX 5070, monitor 1920×1080 a 239 Hz (escala 100%), Kingston NV2 NVMe, Windows 11 25H2. Fixtures em `%TEMP%\fx-fixtures`, com cache quente.

**Carga de fundo nesta rodada:** cerca de 9% de CPU em uso o tempo todo, com Firefox e VS Code abertos (não foram fechados) e o serviço `fx-indexd` antigo rodando.

## Tarefas

| Tarefa | Estado |
|---|---|
| T5.1 Testes manuais | ⏳ parcial (abaixo) |
| T5.2 Distribuição | ⏳ releases publicados pelo CI (D-075), winget preparado; faltam a assinatura (SignPath) e o MSIX, que dependem de contas do usuário |
| T5.3 Site, guia e vídeo | ⏳ site no ar (https://heapwraith.github.io/fx/) e guia publicado; falta o vídeo |
| T5.4 Terminal completo | ✅ |
| T5.5 Pendências de funcionalidades | ✅ |
| T5.6 Robustez | ⏳ serviço desatualizado é avisado e atualizado em Configurações, o desinstalador o remove e as sobras `.fx-old` somem; falta a compilação ARM64 local |
| T5.7 Metas que reprovavam | ⏳ S1, S2, P8, P8b, P8c, P10 e P13 passam; a P6 num disco real espera o Administrador |

## Testes manuais (`docs/TESTES-FINAIS.md`)

Feitos: Narrador, Explorador, gerenciador padrão (aplicado e revertido, registro idêntico) e alto contraste, na T5.1. Feitos agora:
- T2.7: totais pelo serviço em `C:\` (1,5 s).
- T2.11: beta publicado.
- T4.1/T4.4: atualização real.
- T4.6: primeira abertura, simulada com pastas de dados vazias.
- Quick Look de PNG, JPEG, GIF, WebP e TIFF, conferido depois da redução de tamanho.

Faltam, todos fora do alcance deste PC ou dependentes do usuário:

| Item | Por quê |
|---|---|
| T2.9 visualizadores de Office e mídia | este PC não tem Office nem visualizador de mídia registrado (só o de PDF, já testado) |
| T2.10/T4.3 instalador num Windows limpo | precisa de outra máquina ou de uma VM; só o modo isolado (`--test-root`) foi testado |
| T2.11 assinatura | inscrição na SignPath, feita pelo usuário |
| T3.6 ARM64 | os instaladores ARM64 estão no release, mas não há máquina ARM |
| T3.9 OneDrive real | sem OneDrive configurado |
| T4.3 por cima do Inno | exigiria reinstalar a 0.2.0 antiga sobre a instalação real do usuário |
| DPI por monitor | exige trocar a escala da tela do usuário |

## Metas (medidas hoje)

| Meta | Resultado | Limite | Passou? |
|---|---|---|---|
| P1 primeira abertura | mediana 420–436 ms, p95 435–518 ms em 6 rodadas | ≤ 450 ms (p95) | ⚠ oscila no limite (4 de 6 rodadas acima no p95) |
| P2 pasta de 100 mil (fx aberto) | mediana 75–78 ms, p95 83–88 ms; aba nova 9,1 ms (p95 11,3) | ≤ 80 ms / ≤ 16 ms | ⚠ a pasta pedida passa na mediana e reprova no p95; a aba nova passa |
| P3 ordenar e filtrar | nome 37–39 ms (uma rodada com 51 ms); filtro 0,92 ms | ≤ 50 / ≤ 5 ms | ✅ |
| P4 tecla → quadro | digitação p95 4,66; navegação 3,49; caminho 4,81/7,04; abas 5,17; paleta 3,42; busca 3,76; terminal 3,56; durante cópia 4,12 ms | p95 ≤ 8, máx. ≤ 16 ms | ✅ |
| P5 busca | pelo serviço real (todos os volumes NTFS): p95 5,34 ms; índice sintético de 2 milhões: p95 7,05 ms | ≤ 16 ms | ✅ |
| P6 construir o índice | não medido: precisa de Administrador. Leitura da MFT em pedidos grandes (D-076); antes/depois prontos em `%TEMP%\pathdash-p6\p6.ps1` | ≤ 10 s para 2 milhões | ❓ |
| P7 rolagem | 0,02% e 0,10% (4 quadros num pico na partida) | ≤ 0,1% | ✅ |
| P7 rolagem durante cópia de 10 mil arquivos | 0,13%, 0,21%, 0,28% (em 21/11: 0,026%) | ≤ 0,1% | ❌ |
| P8 memória parado | 75,4 MB; por aba 0,46 MB; por janela 11,0 MB | ≤ 85 / 3 / 20 MB | ✅ |
| P9 índice por milhão | 71,4 MB (sintético) | ≤ 100 MB | ✅ |
| P10 cópia | 3.039 MiB/s, 96% da cópia do sistema | ≥ 90% | ✅ |
| P13 instalar e atualizar | 0,90 s / 0,91 s | ≤ 3 / ≤ 2 s | ✅ |
| S1–S3 tamanhos | `pathdash.exe` 14,8 MB (antes 18,6, D-077); instalador 7,8 MB; `fx-indexd` 0,57 MB; `fx-preview` 0,19 MB | ≤ 20 / 10 / 1 / 0,5 MB | ✅ |
| Quick Look | p95 46,6 e 54,6 ms (uma rodada não registrou teclas) | ≤ 60 ms | ✅ (o `fx-bench` cobrava o máximo de 16 ms da P4; corrigido) |
| Menu de contexto | mediana 64,6 ms, p95 397 ms (21/11: 259; 23/11: 344) | ≤ 150 ms | ❌ (já reprovava antes) |
| Unidades | máximo 53 ms | ≤ 1 s | ✅ |

**P1 e P2 contra o executável antigo:** o `pathdash.exe` de antes da redução de tamanho, medido na mesma hora, deu os mesmos números (P2 ~75 ms). Na rodada da manhã, antes desta carga de fundo, a P2 tinha dado 58 ms. A oscilação vem da máquina, e não das mudanças de hoje.

## Riscos

1. **Rolagem durante cópia (P7) reprova hoje** (0,13–0,28%, contra 0,026% em 21/11). Pode ser a carga de fundo, o serviço do índice processando o diário de 10 mil arquivos novos ou o antivírus. Não foi isolado: parar o serviço precisa de Administrador. Próximo passo: medir com o serviço parado e, se continuar, perfilar os quadros perdidos.
2. **Menu de contexto p95 de 260–400 ms** em todas as rodadas desde 21/11. O tempo é das extensões de shell de terceiros na primeira chamada; a mediana é de 56–65 ms. O alvo de 150 ms no p95 provavelmente pede pré-aquecer as extensões na abertura, ou rever a meta.
3. **P1 e P2 oscilam no limite** com o PC carregado. Isso pede medir com o PC ocioso antes de decidir se precisam de trabalho.
4. **P6 sem medição em disco real** desde a Fase 2 (744 mil itens em 7,5 s). A mudança da D-076 corta o custo das chamadas, mas o número real depende do Administrador.
5. **Sem assinatura:** o SmartScreen avisa "editor desconhecido", e o portão pede pacote assinado.

## Recomendação

**Não passar ainda.** O portão pede a 1.0 assinada, com as metas dentro do limite e os testes manuais feitos. Hoje faltam:
- a assinatura e a P6, que dependem do usuário;
- duas metas reprovadas (rolagem durante cópia e menu de contexto);
- os testes que pedem outra máquina.

Próximos passos sem depender do usuário: investigar a rolagem durante cópia e o p95 do menu. Com o usuário: um clique de Administrador para a P6 e para atualizar o serviço, e a inscrição na SignPath. A decisão é do usuário.
