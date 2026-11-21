# Roteiro de comparação manual (T0.10)

O Explorador foi medido automaticamente (`bench/explorer-open.ps1` e `bench/explorer-navigate.ps1`, por UI Automation). O **Files** e o **Everything** não estão instalados nesta máquina; instale-os se quiser a comparação completa e siga este roteiro. Use sempre a mesma máquina e as pastas de `%TEMP%\fx-fixtures` (gere com `fx-bench fixtures`).

## Preparação

1. Feche apps pesados. Ligue o monitor na taxa mais alta (o seu é de 239 Hz).
2. Gravação: qualquer gravador de tela a **120 fps ou mais** (Barra de Jogo do Windows: Win+Alt+R; ou OBS com 120 fps). Mostre o cursor.
3. Para cada medida, faça **10 repetições** e anote todas. Reporte a **mediana** e o **p95** (com 10 valores, o p95 é o maior).
4. Antes de cada série, abra a pasta uma vez e feche (cache quente, como nas medidas do fx).

## Medidas

| # | Medida | Como fazer | Onde parar o cronômetro |
|---|---|---|---|
| 1 | Files: abrir a `flat-100k` com o app fechado | Win+R → `files.exe "%TEMP%\fx-fixtures\flat-100k"` (ou abra o Files e cole o caminho) | Primeiro quadro em que aparece qualquer nome de arquivo na lista |
| 2 | Files: navegar até a `flat-100k` com o app aberto | No Files aberto em `fx-fixtures`, dê dois cliques em `flat-100k` | Primeiro quadro com nomes da nova pasta |
| 3 | Files: lista completa | Mesma navegação da 2 | Quando a contagem "100.000 itens" aparece |
| 4 | Everything: busca por tecla | Com o índice pronto, digite `readme` letra a letra na caixa de busca | Para cada letra: do quadro em que a letra aparece na caixa até o quadro em que a lista muda |
| 5 | Explorador (opcional, confere o automático) | Mesmo procedimento da 1 e da 2, com `explorer.exe` | Mesmo critério |

**Como ler o vídeo:** abra no editor que mostra o número do quadro (ex.: Shotcut, DaVinci Resolve ou `ffprobe`). Tempo = (quadro final − quadro inicial) ÷ fps. O quadro inicial é o primeiro em que o clique, o Enter ou a tecla tem efeito visível (o botão afunda, a letra aparece).

## Onde registrar

Preencha a tabela "Comparação" de `bench/results/gate-0.md` e a seção "Benchmarks" do `README.md`, anotando o método ("vídeo a N fps").
