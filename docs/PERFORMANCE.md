# Desempenho: metas e protocolo de medição

Os resultados mais recentes estão no relatório do último portão ([`bench/results/gate-4.md`](../bench/results/gate-4.md)).

## Metas

Revisadas em 2026-11-22 (D-064). **Limite** reprova um portão; **alvo** é onde queremos chegar.

### Velocidade

| ID | Medida | Limite | Alvo |
|---|---|---|---|
| P1 | Primeira abertura do dia (processo novo) até a primeira lista pintada | ≤ 450 ms p95 | 300 ms |
| P2 | Pasta com 100 mil arquivos, com o fx já carregado: do pedido até a primeira pintura / aba nova | ≤ 80 ms p95 / ≤ 16 ms | 50 ms |
| P3 | Lista de 100 mil ordenada por nome | ≤ 50 ms p95 | 30 ms |
| P4 | Tecla ou clique → quadro atualizado | ≤ 8 ms p95, máximo 16 ms | 4 ms |
| P5 | Busca por nome no disco inteiro, por tecla, ~2 milhões de arquivos | ≤ 16 ms p95 | 8 ms |
| P6 | Construir o índice da MFT de ~2 milhões de arquivos | ≤ 10 s | 6 s |
| P7 | Rolagem na lista de 100 mil, na taxa real do monitor | ≤ 0,1% de quadros perdidos | 0 |
| QL | Quick Look: tecla → prévia na tela | ≤ 60 ms p95 | 40 ms |
| P10 | Copiar um arquivo grande | ≥ 90% da velocidade do Explorador na mesma cópia | 100% |
| P11 | Abrir Configurações, paleta, menus | ≤ 16 ms | — |
| P12 | Arquivo novo aparecer na busca do disco inteiro | ≤ 300 ms | 150 ms |
| P13 | Instalar / aplicar uma atualização ao fechar | ≤ 3 s / ≤ 2 s | — |

### Memória e tamanho

| ID | Medida | Limite | Alvo |
|---|---|---|---|
| P8 | Memória do app parado, 1 janela, 1 pasta pequena | ≤ 85 MB | 70 MB |
| P8b | Cada aba a mais | ≤ 3 MB | 1 MB |
| P8c | Cada janela a mais | ≤ 20 MB | 12 MB |
| P9 | Memória do índice por milhão de arquivos | ≤ 100 MB | 70 MB |
| S1 | `fx.exe` | ≤ 20 MB (D-069) | 15 MB |
| S2 | Instalador completo | ≤ 10 MB | 7 MB |
| S3 | `fx-indexd.exe` / `fx-preview.exe` | ≤ 1 MB / ≤ 0,5 MB | — |

**Hoje reprova** (trabalho da Fase 5, T5.7): P6 (~20 s estimados para 2 milhões; medir pede Administrador). P8 (76 MB) e P8c (11,7 MB por janela) passaram com a D-073. Medidas pelo `fx-bench`: `ship` (S1–S3, P13), `memory-more` (P8b, P8c) e `copy-speed` (P10, contra a cópia do sistema, `CopyFileExW`).

## Protocolo

- **Sempre `--release`.** Números de build debug não valem.
- **Máquina:** a mesma para todas as comparações; anotar CPU, RAM, tipo de SSD e versão do Windows no relatório.
- **Frio vs. quente:** medir as duas situações. "Frio" = primeira abertura após reiniciar o app (o cache de disco do Windows pode estar quente; anotar isso). Para frio real, reiniciar a máquina e anotar.
- **Repetições:** 10 execuções, reportar mediana e p95. Descartar a primeira só se for medida de "quente".
- **Comparação:** medir Explorador, Files e Everything na mesma máquina, na mesma pasta, e registrar o método (cronômetro manual em vídeo a 120 fps vale para o Explorador, que não expõe métrica).

## Fixtures (geradas por `fx-bench fixtures`)

Em `%TEMP%\fx-fixtures\`:

| Pasta | Conteúdo |
|---|---|
| `flat-100k` | 100.000 arquivos vazios numa única pasta, nomes variados (números, Unicode, extensões mistas) |
| `flat-10k-media` | 10.000 imagens pequenas (PNG de 64×64 gerados) para testar miniaturas |
| `deep-tree` | árvore com 20 níveis e 50.000 arquivos no total |
| `unicode-long` | nomes com acentos, emoji, CJK e caminhos > 260 caracteres |

A geração precisa ser determinística (semente fixa) para os resultados serem comparáveis.

## Saída

Cada benchmark grava `bench/results/<data>-<id>.json` com: id da meta, mediana, p95, número de itens, máquina, commit. `/bench` lê esses arquivos e imprime uma tabela "meta × resultado × passou?".
