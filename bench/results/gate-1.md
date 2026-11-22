# Portão da Fase 1 — relatório

**Data:** 2026-11-21 · **Commit de referência:** `main` após a T1.17 · **Build:** release (`lto = "fat"`), compilado no WSL com `cargo xwin`, executado no Windows.

## Máquina

Mesma da Fase 0: AMD Ryzen 7 5700X, 32 GB, RTX 5070, monitor 1920×1080 a 239 Hz (escala 100%), Kingston NV2 NVMe, Windows 11 25H2 (10.0.26200). Fixtures em `%TEMP%\fx-fixtures`, cache de disco quente. Extensões de shell instaladas: PowerToys, 7-Zip, Microsoft Defender, Clipchamp e outras (pesam no menu de contexto).

## Tarefas da fase

| Tarefa | Estado |
|---|---|
| T1.1 Fundação visual | ✅ (capturas aprovadas pelo usuário) |
| T1.2 Seleção, teclado, instância única | ✅ |
| T1.3 Histórico, migalhas, autocompletar | ✅ |
| T1.4 Abas | ✅ (arrastar aba e clique do meio não exercitados na janela) |
| T1.5 Lateral | ✅ (pendrive real não testado) |
| T1.6 Ícones | ✅ |
| T1.7 Grade e miniaturas | ✅ |
| T1.8 Observador de mudanças | ✅ |
| T1.9 Abrir arquivos | ✅ (não abrimos apps reais pela janela) |
| T1.10 Acessibilidade (AccessKit) | ✅ árvore UIA verificada · **Narrador ao vivo pendente com o usuário** |
| T1.11 Motor de cópia/movimentação | ✅ |
| T1.12 Lixeira, renomear, nova pasta, desfazer | ✅ |
| T1.13 Fila e conflitos | ✅ (diálogo testado pelo motor, não clicado) |
| T1.14 Área de transferência | ✅ · **teste manual com o Explorador pendente** |
| T1.15 Arrastar e soltar | ✅ entre dois fx · **Explorador, área de trabalho e navegador pendentes (manual)** |
| T1.16 Menu de contexto | ✅ funcional · **p95 acima da meta neste PC** (ver abaixo) |
| T1.17 Persistência | ✅ |
| T1.19 Revisão visual | ✅ |
| T1.20 Ordenar por coluna, Tipo, larguras | ✅ · P4 com reordenação de 100 mil: p95 ≤ 4,4 ms |
| T1.21 Ocultos e Propriedades | ✅ · Propriedades corrigidas na conferência (D-034) |
| T1.22 Quick Look | ✅ · foto de 9,5 MB: ≤ 69 ms (meta 100) |
| T1.23 Busca no disco (Ctrl+F) | ✅ · P4 com resultados chegando: p95 ≤ 5,3 ms |
| T1.24 Paleta de comandos | ✅ · P4: p95 ≤ 4,7 ms |
| T1.18 Relatório do portão | este documento · **falta o registro das 2 semanas de uso** |

## Critérios do portão × resultados

| Critério | Resultado | Passou? |
|---|---|---|
| **1. Duas semanas como gerenciador principal**, cada volta ao Explorador anotada | Ainda não começou: depende do usuário | ⏳ |
| **2. P1** primeira abertura do dia (≤ 500 ms) | mediana 411–430 ms, p95 423–469 (Fase 0: 372–406) | ✅ (pior que a Fase 0, dentro da meta) |
| **2. P2 (b)** pasta de 100 mil com o fx carregado (≤ 100 ms) | aba nova (Ctrl+T, D-013): **mediana 10,2 ms**, p95 11,7. Processo novo, do pedido da pasta: 61–72 ms (Fase 0: 24) | ✅ |
| **2. P3** lista completa e ordenada, 100 mil (≤ 300 ms) | ordenar 100 mil: 37 ms; da partida até carregada: ~430 ms; pasta já com o fx aberto: lida e ordenada em ~90 ms (mesmo caminho da Fase 0) | ✅ |
| **2. P4** tecla → quadro (≤ 16 ms p95) | filtro p95 3,6 ms; setas p95 3,3; Enter/Backspace 2,9; abas 5,1; campo de caminho 3,35 (T1.3); com cópia de 10 mil em andamento 4,0 (T1.13) | ✅ |
| **2. P7** rolagem, ≤ 0,5% de quadros perdidos (D-014) | **0 perdidos em 4.001** quadros (arraste + roda, 100% da lista); grade com miniaturas: 0 em 3.999; durante cópia: 1 em 3.904 | ✅ |
| **3. Nenhuma perda de dados** | Nenhum dado do usuário perdido. **Um arquivo de teste** (criado pelo próprio teste em `fx-fixtures`) foi excluído de vez pelo Windows na T1.12, ao enviar para a Lixeira por uma unidade `subst` sem Lixeira. Corrigido antes do commit: caminho canonizado e recusa quando a unidade não tem Lixeira (D-022), com teste. | ✅ com ressalva |

### Outras metas

| Meta | Resultado | |
|---|---|---|
| P5 busca por tecla (≤ 50 ms) | índice sintético de 2M: p95 8,75 ms (real do C: na Fase 0: 4,1 ms) | ✅ |
| P8 memória parado, 1 janela, **pasta pequena** (≤ 80 MB) | **83,9–84,6 MB** privados (a versão da Fase 0, medida hoje do mesmo jeito: 72,1). Com a `flat-100k` aberta: 97 MB (a lista de 100 mil custa ~13 MB, igual na Fase 0) | ❌ por ~4 MB |
| P9 índice por milhão (≤ 150 MB) | 71,4 MB/milhão (sintético) | ✅ |
| Menu de contexto (T1.16, p95 ≤ 150 ms) | mediana 64 ms, **p95 259 ms** (extensões do shell na 1ª abertura de cada tipo; repetidos 41–69 ms) | ❌ neste PC |
| Unidade nova na lateral (≤ 1 s) | 23–60 ms | ✅ |
| Persistência sob processo morto | banco íntegro e último estado completo em 5 de 5 mortes | ✅ |

## Comparação com o Explorador (medida hoje, mesma máquina)

| Medida (`flat-100k`, 100 mil arquivos) | fx | Explorador |
|---|---|---|
| Abrir com o programa já aberto | **10 ms** (aba nova) | **254 ms** mediana, p95 288 (navegar na mesma janela) |
| Abrir numa janela nova | 411–430 ms (processo novo; com a instância única, uma segunda janela é só a aba/janela nova: ~10 ms) | **1.105 ms** mediana, p95 1.192 |
| Menu de contexto | mesmas extensões do shell: custo equivalente | — (não medido) |

Método igual ao da Fase 0: fx pelo `FX_TRACE`, Explorador por UI Automation (`bench/explorer-open.ps1`, `bench/explorer-navigate.ps1`, resolução de ~5–15 ms). **Files e Everything** continuam não instalados; o roteiro está em `bench/ROTEIRO-COMPARACAO.md`.

## Riscos encontrados

1. **Memória (P8):** 83,9 MB parado numa pasta pequena (a definição da P8), ~4 MB acima da meta. Medição por versão, nas mesmas condições: Fase 0 72,1 → lateral +3,3 → ícones +6,5 → AccessKit +1,5 → resto +0,6 MB. Threads e COM não são o custo (iniciar as miniaturas só na grade tirou 4 threads e nada de memória). Os 95,7 MB citados numa primeira versão deste relatório eram da pasta de 100 mil arquivos, que a P8 não usa. Próximo passo: perfilar heap, imagens de DLL e alocações do driver (VMMap ou ETW) nas partes de ícones e lateral.
2. **Menu de contexto:** o p95 depende das extensões instaladas, e neste PC passa da meta. O fx já não espera o menu na thread de UI e tem tempo limite contra extensões travadas; o que sobra é custo do shell, igual no Explorador.
3. **Processo novo mais lento:** do pedido da pasta ao primeiro quadro, 24 → 61–72 ms, porque o primeiro quadro de um processo novo tem mais o que desenhar. Não afeta a P2 (b) do fx residente (10 ms), mas a P1 também subiu (~40 ms).
4. **Residente sem janela (D-013):** fechar a última janela ainda encerra o processo. Para valer o "primeira abertura do dia", falta um jeito de o fx ficar carregado (bandeja ou serviço), com opção de sair.
5. **Patch do GPUI cresceu:** além do Mica, agora uma correção de crash no arrastar e soltar (`DragEnter`). Mais um ponto para reaplicar ao atualizar o GPUI.
6. **Testes manuais pendentes:** Narrador ao vivo (T1.10), copiar e colar e arrastar com o Explorador, a área de trabalho e o navegador (T1.14 e T1.15). São a forma mais rápida de achar problemas nas 2 semanas de uso.

## Recomendação

**Começar as 2 semanas de uso diário agora, sem passar o portão ainda.** Os critérios numéricos do portão (P1–P4 e P7) passam e o fx segue bem à frente do Explorador. Antes de decidir:

1. Usar o fx como gerenciador principal e anotar cada volta ao Explorador (com o motivo); cada uma vira tarefa.
2. Fazer os testes manuais da lista acima nos primeiros dias.
3. Em paralelo, perfilar e reduzir a memória (P8), ~4 MB acima da meta.

A decisão de passar o portão é do usuário.
