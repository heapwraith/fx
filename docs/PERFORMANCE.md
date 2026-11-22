# Desempenho: metas e protocolo de medição

Os resultados mais recentes estão no relatório do último portão ([`bench/results/gate-4.md`](../bench/results/gate-4.md)). Todas as metas passam; a P8 (memória parado) está no limite.

## Metas

| ID | Medida | Meta |
|---|---|---|
| P1 | Primeira abertura do dia (processo novo) até a primeira lista pintada | ≤ 500 ms (revisada em D-013; era 150 ms) |
| P2 | Pasta com 100 mil arquivos: primeira pintura, com o fx já carregado (processo residente, D-013) | ≤ 100 ms |
| P3 | Pasta com 100 mil arquivos: lista completa e ordenada por nome, com o fx já carregado | ≤ 300 ms |
| P4 | Resposta a tecla/clique (input → quadro atualizado) | ≤ 16 ms |
| P5 | Busca por nome no disco inteiro, por tecla, com ~2 milhões de arquivos indexados | ≤ 50 ms |
| P6 | Construção inicial do índice da MFT de um disco com ~2 milhões de arquivos | ≤ 10 s |
| P7 | Rolagem na lista de 100 mil itens, na taxa real do monitor | ≤ 0,5% de quadros perdidos (intervalo ≥ 1,5× o período do monitor); reportar também o p95 (D-014) |
| P8 | Memória do app parado, 1 janela, 1 pasta pequena | ≤ 88 MB (revisada em D-038; era 80 MB) |
| P9 | Memória do índice por milhão de arquivos | ≤ 150 MB |

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
