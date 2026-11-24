# Guia de uso do Pathdash

O Pathdash é um gerenciador de arquivos para Windows 10 e 11. Tudo se faz pelo teclado; na dúvida, **Ctrl+K** abre a paleta, que encontra qualquer comando ou pasta pelo nome. **F1** mostra o resumo dos atalhos dentro do próprio Pathdash.

## Instalar

Baixe o instalador (`Pathdash-<versão>-setup-x64.exe`, ou `-arm64` em PCs ARM) e abra. Ele instala só para o seu usuário, sem senha de administrador, em poucos segundos. As atualizações chegam sozinhas e são aplicadas quando o Pathdash fecha (pode desligar em Configurações › Atualizações). Para desinstalar: Configurações do Windows › Aplicativos › Pathdash (o desinstalador também oferece remover o serviço de busca).

## Navegar

| Tecla | O que faz |
|---|---|
| Enter | abre a pasta ou o arquivo |
| Alt+↑ ou Backspace | pasta acima |
| Alt+← / Alt+→ | voltar e avançar |
| Ctrl+L | digitar um caminho (com sugestões; Tab completa) |
| F6 | barra lateral pelo teclado |
| letras | filtram a pasta na hora (Esc limpa) |
| Ctrl+B | mostra ou esconde a barra lateral |

A barra de caminho mostra as pastas acima; clicar numa delas vai até lá. Na paleta, digite parte do nome de uma pasta que você abre muito: o Pathdash aprende quais você usa mais e as coloca primeiro ("proj site" acha `C:\Projetos\site-novo`).

## Buscar

**Ctrl+F** busca nesta pasta e nas de dentro; **Tab** troca para o disco inteiro (com o serviço de busca ligado em Configurações › Sistema, a busca no disco inteiro responde enquanto você digita). Junte filtros ao nome:

| Filtro | Exemplo |
|---|---|
| `tipo:` | `tipo:imagem`, `tipo:vídeo`, `tipo:documento` |
| `ext:` | `ext:pdf` |
| `tamanho:` | `tamanho:>10mb` |
| `data:` | `data:semana`, `data:hoje` |
| `etiqueta:` | `etiqueta:vermelho` (ou o nome que você deu à etiqueta) |
| `conteúdo:` | `conteúdo:"nota fiscal"` — lê os arquivos; no disco inteiro, junte parte do nome ou `ext:` |

Nos resultados, Enter abre e Ctrl+Enter mostra o arquivo na pasta dele. **Ctrl+S** salva a busca: ela aparece na barra lateral e na paleta e se atualiza sozinha.

## Arquivos

| Tecla | O que faz |
|---|---|
| F2 | renomear (com vários selecionados: renomear em lote, com prévia) |
| Ctrl+Shift+N | nova pasta |
| Ctrl+C / Ctrl+X / Ctrl+V | copiar, recortar, colar |
| Ctrl+Shift+C | copiar o caminho |
| Delete / Shift+Delete | Lixeira / excluir de vez (pede confirmação) |
| Ctrl+Z | desfaz qualquer operação, mesmo de outro dia |
| Ctrl+Shift+H | histórico de operações |
| Alt+Enter | Propriedades do Windows |
| Shift+F10 | menu de contexto do Windows |

Cópias e movimentos vão para uma fila com progresso real; conflitos de nome perguntam o que fazer, e o padrão é manter os dois. Arrastar e soltar funciona entre o Pathdash, o Explorador e outros programas.

## Ver

| Tecla | O que faz |
|---|---|
| Espaço | Quick Look: prévia de imagens, textos, código, PDF, Office, vídeo, ZIP; setas trocam de arquivo |
| Ctrl+H | mostra arquivos ocultos |
| Ctrl+1 / 2 / 3 | densidade das linhas |
| Ctrl+Shift+3 | vista em colunas |
| Alt+Shift+P | painel de detalhes (propriedades do arquivo) |

O Pathdash abre no tema escuro; em Configurações › Aparência dá para escolher o claro ou seguir o Windows.

Pela paleta: alternar grade e lista, ordenar, agrupar por tipo, data ou tamanho, e as colunas de dimensões, duração e data da foto.

## Ferramentas

| Tecla | O que faz |
|---|---|
| Ctrl+Shift+M | mapa do disco: o que ocupa espaço nesta pasta |
| Ctrl+Shift+L | linha do tempo: o que mudou nos discos |
| Ctrl+Shift+D | arquivos repetidos |
| Ctrl+E | extrair arquivos compactados (ZIP, 7z, tar) |

- **Arquivos compactados:** Enter num ZIP abre como pasta; Ctrl+C copia de dentro dele; "Compactar em ZIP" na paleta cria um.
- **Comparar:** "Comparar arquivos (selecionados)" mostra dois textos lado a lado (N e P pulam entre as diferenças); com o painel duplo, "Comparar com o outro painel" compara pastas.
- **Imagens:** na paleta, copiar como JPG ou PNG, cópia com metade do tamanho e renomear pela data da foto. O original nunca muda.
- **Git:** em pastas de repositórios, cada arquivo mostra uma letra (M, A, ?…).

## Abas, painéis e janelas

| Tecla | O que faz |
|---|---|
| Ctrl+T / Ctrl+W | nova aba / fechar aba |
| Ctrl+Tab | próxima aba |
| F3 | painel duplo; Tab passa de um painel ao outro |
| Ctrl+F3 / Ctrl+Shift+F3 | mais um painel (até 4) / fechar este painel |
| F5 / F6 | no painel duplo: copiar / mover para o outro painel |

Arraste uma aba para a área dos painéis para levá-la a uma janela nova. "Salvar abas como espaço de trabalho…" guarda as abas da janela com um nome, para reabrir depois pela paleta. As abas, os painéis e a vista em colunas voltam como estavam ao reabrir o Pathdash.

## Terminal

**Ctrl+J** abre o PowerShell embaixo da pasta atual (pela paleta, também o Prompt de Comando). Ele acompanha a pasta quando você navega; a roda do mouse rola o histórico; arraste para selecionar e Ctrl+Shift+C para copiar, Ctrl+Shift+V para colar. Ctrl+J fecha.

## Etiquetas

**Alt+1…7** põe uma etiqueta colorida nos itens selecionados (Alt+0 tira). "Renomear etiqueta" na paleta dá um nome a ela (por exemplo, "Urgente"); "Etiqueta: …" na paleta mostra tudo o que tem aquela etiqueta, em qualquer pasta.

## Gerenciador padrão

Em Configurações › Sistema, o Pathdash pode abrir no lugar do Explorador (Win+E e pastas abertas por outros programas). Pastas que não existem no disco (Lixeira, Este Computador, Painel de Controle) continuam abrindo no Explorador. O mesmo lugar desfaz a troca.

## Privacidade

O Pathdash não envia nada: não há telemetria. A única conexão é a verificação de atualizações no GitHub, que pode ser desligada. Relatórios de falha ficam no seu PC e só são enviados se você pedir.
