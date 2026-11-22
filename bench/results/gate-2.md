# Portão da Fase 2 — relatório

**Data:** 2026-11-22 · **Versão:** 0.2.0-beta · **Build:** release, compilado no WSL com `cargo xwin`, executado no Windows; CI verde no GitHub (`windows-latest`).

## Máquina

A mesma das fases anteriores: AMD Ryzen 7 5700X, 32 GB, RTX 5070, monitor 1920×1080 a 239 Hz (escala 100%), Kingston NV2 NVMe, Windows 11 25H2. Discos C: e D: (NTFS), 744 mil itens no índice. Fixtures em `%TEMP%\fx-fixtures`, cache quente.

## Tarefas

| Tarefa | Estado |
|---|---|
| T2.1 Ícone e metadados | ✅ |
| T2.2 Pipeline e versão | ✅ (releases por tag, rascunho) |
| T2.3 Pendências da Fase 1 | ✅ (testes manuais movidos para `docs/TESTES-FINAIS.md`) |
| T2.4 Testes de falha do motor | ✅ 9 testes, nenhum perde dados |
| T2.5 `fx-indexd` | ✅ |
| T2.6 Busca completa | ✅ |
| T2.7 Tamanho das pastas | ✅ (caminho pelo serviço testado com índice sintético) |
| T2.8 Renomear em lote | ✅ |
| T2.9 Prévias completas | ✅ (DOCX/MP4/MP3 sem visualizador nesta máquina) |
| T2.10 Instalador | ✅ (teste em Windows limpo pendente) |
| T2.11 Beta pública | ⏳ parte do agente feita; falta SignPath, testes manuais e publicar (usuário) |
| T2.12 Este relatório | ✅ |

## Critérios do portão

| Critério | Resultado | Passou? |
|---|---|---|
| **1. Nenhum caso de perda de dados**, testes da T2.4 passando | Nenhum caso. Testes: disco cheio, arquivo em uso, somente leitura, mover com origem travada, **processo morto no meio** (10/10), > 4 GiB; diário de temporários limpa o que um processo morto deixou | ✅ |
| **2. P1–P9 mantidas** | ver abaixo: todas dentro da meta | ✅ |
| **3. Beta publicada, taxa de falhas conhecida** | Rascunho `v0.2.0-beta` gerado pelo CI; publicar e coletar falhas depende do usuário. Um travamento real foi achado e corrigido nesta fase (texto truncado com "…", D-038) | ⏳ |

## Metas (medidas hoje)

| Meta | Resultado | Meta | |
|---|---|---|---|
| P1 primeira abertura | mediana 433 ms, p95 439 | ≤ 500 ms | ✅ |
| P2 pasta de 100 mil, fx carregado | aba nova: mediana 10,5 ms, p95 12,0; do pedido da pasta em processo novo: 75/88 ms | ≤ 100 ms | ✅ |
| P3 lista ordenada de 100 mil | sem mudança no caminho (ordenar 100 mil: ~37 ms, Fase 1) | ≤ 300 ms | ✅ |
| P4 tecla → quadro | paleta p95 3,5 ms; setas durante reordenação 3,0; busca com resultados chegando 3,4; abas 3,9 | ≤ 16 ms | ✅ |
| P5 busca no disco inteiro | pelo serviço (pipe + todos os volumes + filtros, até 1000 resultados): p95 **5,39 ms** (744 mil itens) | ≤ 50 ms | ✅ |
| P6 construir o índice | 744 mil itens em 7,5 s (C: e D:) | ≤ 10 s (2 milhões) | ✅ proporcional |
| P7 rolagem | **0 perdidos em 3.999** quadros a 239 Hz | ≤ 0,5% | ✅ |
| P8 memória parado | mediana 85,8 MB | ≤ 88 MB (D-038) | ✅ |
| P9 índice por milhão | 73,5 MB para 744 mil = **98,7 MB/milhão** (estável sob mudanças); serviço inteiro 76 MB | ≤ 150 MB | ✅ |
| Quick Look | foto 9,5 MB p95 46–66 ms; texto/código < 10 ms | ≤ 100 ms | ✅ |
| Arquivo novo na busca | 101–258 ms | ≤ 1 s | ✅ |

## Comparação

| | fx | Referência |
|---|---|---|
| Índice por milhão de arquivos | ~99 MB | Everything ~100 MB (regra do autor, fórum voidtools) |
| Memória de uma janela, pasta pequena | 85,8 MB (app inteiro) | Explorador: +117–125 MB por janela (medido aqui) |
| Abrir pasta de 100 mil com o programa aberto | 10,5 ms | Explorador 254 ms (Fase 1) |

Files e Everything não estão instalados aqui; números públicos e método em `docs/DECISIONS.md` (conversa de 2026-11-22).

## Riscos

1. **Testes manuais adiados** (pedido do usuário): Narrador, copiar/arrastar com o Explorador, instalador num Windows limpo, visualizadores de Office e mídia. Lista em `docs/TESTES-FINAIS.md`.
2. **Sem assinatura** até a inscrição na SignPath: o SmartScreen avisa "editor desconhecido".
3. **Prévias do Windows** rodam numa janela separada mantida sobre o painel (D-044): pode atrasar até 250 ms ao mover a janela e fica sobre menus abertos por cima do Quick Look.
4. **Serviço de índice:** roda como SYSTEM; mitigado (cópia em Program Files, pipe só do usuário, protocolo limitado), mas é a parte mais sensível do projeto. Falta tempo limite para conexões paradas.

## Recomendação

**Passar para a Fase 3**, como o usuário pediu (terminar as funcionalidades e testar no fim), com a beta publicada quando a assinatura e os testes manuais saírem. A decisão é do usuário.
