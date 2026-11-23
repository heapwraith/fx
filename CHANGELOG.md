# Changelog

Todas as mudanças relevantes do fx. Versões seguem o [versionamento semântico](https://semver.org/lang/pt-BR/): enquanto for 0.x, cada fase do projeto sobe o número do meio (Fase 2 → 0.2). O pipeline publica uma versão quando recebe a tag `vX.Y.Z` igual à versão do `Cargo.toml`; as notas do release saem da seção da versão aqui.

## [Não lançado]

### Mudou
- **Novo nome: Pathdash.** O fx passa a se chamar Pathdash, com ícone novo (grafite, com um caminho em ciano que termina numa seta) e o slogan "Qualquer pasta, numa arrancada.". O programa agora é `pathdash.exe`; ao atualizar, o instalador troca o `fx.exe` e os atalhos antigos e, se o fx era o gerenciador padrão, aponta o Win+E para o programa novo. Configurações, abas e etiquetas continuam onde estavam.
- **Cor de destaque ciano** no lugar do verde-azulado; os temas claro e escuro não mudam.
- **Tema escuro por padrão.** Em Configurações › Aparência: Escuro, Claro ou Seguir o Windows, com troca na hora.
- **Tamanhos em MB, GB e TB.** Arquivos grandes não aparecem mais como milhares de KB: `17,6 MB`, `2,5 GB` (abaixo de 1 MB continua em KB, como no Explorador). Vale para a lista, a busca, os totais de pasta, o Quick Look e o Narrador.

### Corrigido
- Com o Pathdash como gerenciador padrão, abrir uma unidade (`D:\`) pelo Windows mostrava um caminho quebrado (`D:"`): a barra final engolia a aspa da linha de comando.

## [0.3.0-beta] — 2026-11-23

Fases 5 e 6 (em andamento): o que nenhum outro gerenciador tem junto (mapa do disco, linha do tempo, buscas vivas, duplicados, desfazer entre sessões), compactados, busca no conteúdo, comparar pastas e arquivos, dependências que rodam no Windows e no Linux, instalador menor e menos memória.

### Adicionado
- **Avisos de terceiros:** o instalador traz `THIRD-PARTY-NOTICES.txt`, com a licença de cada biblioteca usada.
- **Terminal completo:** acompanha a pasta da aba, Prompt de Comando além do PowerShell, histórico com a roda do mouse, selecionar com o mouse, copiar e colar.
- **Sessão com painéis:** os painéis lado a lado e a vista em colunas voltam como estavam ao reabrir o fx.
- **Etiquetas com nome próprio**, visíveis também na grade e nas colunas, e "Etiqueta: …" na paleta para achar tudo o que tem uma etiqueta.
- Ícones de verdade nos resultados da busca e na vista em colunas; Espaço abre o Quick Look quando o filtro deixa um item só.
- **Instalador próprio** (fx-setup), com a interface do fx: instala só para o usuário, troca os arquivos de uma vez (volta ao que havia se algo falhar) e desinstala sem deixar nada. Substitui o Inno Setup.
- **Atualizações automáticas:** a versão nova é baixada e conferida (SHA-256) em segundo plano e aplicada ao fechar o fx; "Atualizar o fx" na paleta reinicia na hora. Pode ser desligado.
- Telas **"Sobre o fx"** e **"Como usar o fx"** (F1; abre sozinha na primeira vez).
- **Gerenciador padrão:** pastas, unidades e Win+E abrem no fx; "Voltar ao Explorador" restaura tudo.
- **Painel duplo** (F3): Tab troca de painel, F5 copia e F6 move para o outro.
- **Vista em colunas** (Ctrl+Shift+3).
- **Etiquetas coloridas** (Alt+1…7) e busca por `etiqueta:`.
- **Terminal embutido** (Ctrl+J) com PowerShell na pasta atual.
- **Pastas de nuvem** (OneDrive e outras): estado de cada item e comandos para manter ou liberar espaço.
- Interface em **português e inglês** (idioma do Windows; troca na paleta).
- **Alto contraste** do Windows com as cores do sistema, ao vivo.
- **Relatório de falhas** local e anônimo, com issue pré-preenchida (nada é enviado sozinho).
- Build e instalador para **Windows ARM64**.
- Transições curtas nas sobreposições (respeitam "Efeitos de animação" do Windows) e mensagens claras para busca e paleta sem resultado.

### Mudado
- **Menos memória:** o fx parado usa 76 MB em vez de 88 MB, e cada janela a mais custa 12 MB em vez de 24 MB.
- **Instalador com menos da metade do tamanho:** 9,8 MB em vez de 16,5 MB (um inicializador pequeno abre o instalador de dentro do pacote).
- **Compactados sem programas do Windows** (D-069): ZIP, 7z e tar são lidos e escritos por bibliotecas Rust, as mesmas no Windows e no Linux, dentro do próprio fx; acabou o PowerShell oculto e o `tar.exe`. RAR deixou de ser aberto (não há leitor livre em Rust).
- **Imagens sem o WIC** (D-069): converter e reduzir por bibliotecas Rust, várias fotos ao mesmo tempo (em lote, 44 ms por foto de 12 MP, metade do tempo de antes), já de pé conforme a orientação da câmera; a data da foto vem direto do EXIF. HEIC deixou de ser convertido.
- **Fuso horário pelas regras IANA** (D-069): as datas da lista seguem as mesmas regras no Windows e no Linux, iguais às do Windows nos anos recentes.
- **Busca no conteúdo sem o Windows Search** (D-069): no disco inteiro, o índice do fx acha os arquivos pelo nome ou extensão e o fx lê o conteúdo deles; só `conteúdo:` sem nome nem extensão pede um dos dois em vez de ler o disco todo.
- Delete pergunta antes de enviar para a Lixeira (dá para desligar na paleta); as confirmações usam um diálogo do fx, traduzido, em vez da caixa do Windows.
- Barra de progresso mais visível, com porcentagem, velocidade e tempo restante; a operação concluída fica um instante na tela.
- **Configurações** (Ctrl+, ou a engrenagem no rodapé da barra lateral): idioma, Lixeira, sessão, aparência, atualizações, privacidade, sistema e "Sobre o fx". O rodapé da lateral também tem "Ajuda" (F1).
- A vista em colunas mostra sempre a coluna da pasta atual inteira; faixas nas bordas mostram quantas colunas estão escondidas e trazem a próxima com um clique.
- **Leitores de tela** (Narrador, NVDA): diálogos de confirmação, conflito, paleta, Configurações, guia, histórico, busca, Quick Look, renomear em lote, menu Exibir, terminal e vista em colunas agora existem para eles, com o foco no item ativo e "x de N" nas listas; o clique do leitor de tela aciona os botões.
- **Configurações pelo teclado:** Tab passa entre os controles, Espaço usa, ↑ e ↓ trocam de seção; o anel de foco aparece ao usar o teclado. Os interruptores respondem ao clique em qualquer parte da linha.
- **"Ir para" que aprende:** a paleta (Ctrl+K) ordena as pastas pelo seu uso real, frequência e recência, como o zoxide; "proj fx" leva à pasta `Projetos\fx` mais usada.
- **Espaços de trabalho:** "Salvar abas como espaço de trabalho…" na paleta guarda as abas da janela com um nome; "Espaço: Trabalho" abre esse conjunto numa janela nova.
- **Desfazer entre sessões:** o histórico de operações sobrevive ao fechar o fx; dá para desfazer a limpeza de ontem pelo painel de histórico (mostra a data), sem nunca sobrescrever nada.
- **Mapa do disco** (Ctrl+Shift+M): o que ocupa espaço na pasta, em blocos proporcionais; Enter entra numa pasta, Delete libera espaço (com confirmação e desfazer).
- **Linha do tempo** (Ctrl+Shift+L): o que foi criado, alterado, renomeado ou apagado nos discos nos últimos 10 minutos, na última hora ou no dia (com a busca no disco inteiro ativada).
- **Buscas vivas:** Ctrl+S no painel de busca salva a busca; ela fica na barra lateral e na paleta e, no disco inteiro, se atualiza sozinha enquanto aberta.
- **Arquivos repetidos** (Ctrl+Shift+D): acha cópias idênticas na pasta e subpastas, comparando o conteúdo de verdade, e manda as marcadas para a Lixeira com desfazer.
- **Painel de detalhes** (Alt+Shift+P): dimensões, duração, data da foto, câmera, autor e outros dados do item, como o Windows os mostra.
- **Agrupar por e colunas de mídia** (menu Exibir, paleta): agrupar por tipo, data ou tamanho, com cabeçalhos na lista; colunas Dimensões, Duração e Data da foto.
- **Arquivos compactados:** Enter num ZIP abre como pasta; Ctrl+E extrai (também 7z e tar, com gzip, bzip2 ou xz), Ctrl+C copia de dentro dele e "Compactar em ZIP" na paleta cria um, tudo com progresso real, conflitos perguntando e desfazer.
- **Busca no conteúdo:** `conteúdo:"nota fiscal"` na busca acha os arquivos que contêm o texto — nesta pasta lendo os arquivos; no disco inteiro, lendo os que o índice do fx acha pelo nome ou pela extensão.
- **Abas e painéis:** soltar uma aba sobre os painéis a leva para uma janela nova (também pela paleta); Ctrl+F3 divide em até 4 painéis, Ctrl+Shift+F3 fecha o painel.
- **Comparar arquivos** (paleta): dois textos lado a lado com as linhas que mudaram, N e P pulando entre trechos; outros arquivos dizem se são idênticos ou onde diferem; três ou mais ficam agrupados pelos idênticos.
- **Status do Git:** em pastas de repositórios, cada item mostra se está alterado (M), novo (?), adicionado (A), renomeado (R), apagado (D) ou em conflito (!).
- **Comparar pastas:** com o painel duplo, "Comparar com o outro painel" mostra o que difere entre as duas pastas e copia para um lado ou outro, perguntando nos conflitos.
- **Ações rápidas com imagens** (paleta): copiar as imagens selecionadas como JPG ou PNG, ou com metade do tamanho (o original nunca muda), e renomear fotos pela data em que foram tiradas, com desfazer.
- **Barra lateral pelo teclado (F6)** e dicas com a ação e o atalho nos botões que são só um ícone.
- Paleta centralizada e com um ícone por comando; guia "Como usar" com botão de fechar, rodapé sempre visível e teclas separadas; a busca mostra o que digitar antes da primeira letra.
- Registro de avisos e erros em `%LOCALAPPDATA%\fx\logs\fx.log` (local, até 1 MB), para diagnosticar problemas.
- Esperas da interface não ocupam mais o pool de threads do Windows; a atualização roda numa thread própria.

### Corrigido
- A prévia de PDF e Office no Quick Look não cobre mais a paleta e os menus do fx, e acompanha a janela ao arrastar.
- F6 no painel duplo volta a mover a seleção para o outro painel (antes ia para a barra lateral).
- Desinstalar não deixa mais uma cópia do desinstalador na pasta temporária.
- O serviço de busca no disco não fica mais com uma conexão presa para sempre quando um programa conecta e não fala nada.
- Abrir uma pasta grande voltou a ser mais rápido (P2: mediana de 82 para 73–78 ms em 100 mil arquivos), desfazendo a espera dos lotes por uma thread intermediária.
- A busca "nesta pasta" e os totais entram em pastas do OneDrive (antes eram tratadas como links).
- Terminal, botões de perigo e etiquetas seguem as cores do alto contraste.
- "Compactar em ZIP" criava `nome (2).zip` mesmo sem existir `nome.zip`.
- **Revisão de código completa (D-066):**
  - Enter segurado não confirma mais um diálogo (a paleta podia confirmar "Excluir permanentemente" sem ser vista); diálogos e sobreposições bloqueiam o mouse; as Configurações não deixam mais Delete chegar à lista escondida; excluir pelo menu de contexto também pede confirmação; na vista em colunas, Delete age no item visível.
  - O atualizador reconfere o instalador (hash e editor da assinatura) na hora de aplicar e nunca roda elevado; a assinatura passa a exigir o mesmo editor e checar revogação.
  - Os pipes de instância única e do `fx-indexd` ficam restritos ao usuário; um cliente parado não trava mais a busca no disco inteiro.
  - Com o fx como padrão, Lixeira, Este Computador e Painel de Controle voltam a abrir (no Explorador).
  - Copiar ou mover um arquivo cujo nome não é Unicode criava o destino com outro nome; desfazer um renomear ou um envio para a Lixeira desses nomes também falhava.
  - Nomes muito longos falhavam ao copiar (o temporário passava de 255 caracteres).
  - O instalador dizia "nenhum arquivo foi alterado" mesmo depois de trocar os arquivos.
  - Fechar o terminal podia congelar a janela no Windows 10.
  - Etiquetas se perdiam ao mover pastas com certos caracteres (como "İ").
  - Velocidade e tempo restante contavam o tempo pausado; a barra ficava vazia ao copiar uma pasta vazia.
  - "3 itens itens"; escolher o idioma já em uso trocava o idioma; F1 com as Configurações abertas abria o guia atrás delas.
  - SQLite atualizado (rusqlite 0.43) por causa de uma vulnerabilidade do SQLite embutido (RUSTSEC-2026-0459).
- O diálogo de conflito deixava as teclas chegarem à lista atrás dele; agora Enter mantém os dois e Esc pula.
- O arquivo temporário de uma cópia em andamento aparecia na lista.
- O progresso do instalador tinha etapas simuladas; agora mostra só o trabalho feito.
- A lista aparecia pelas bordas do Quick Look e da busca; contagem de selecionados sem separador de milhar; textos cortados em inglês.

- Mensagens de erro de cópia, Lixeira, menu e Propriedades nos dois idiomas.
- As bibliotecas de rede do atualizador só carregam quando ele roda (menos memória na abertura).

## [0.2.0-beta] — 2026-11-22

Fase 2 — busca no disco inteiro, prévias e instalador. Primeira beta pública.

### Adicionado
- Ícone do fx e versão nas Propriedades do executável (T2.1).
- Pipeline no GitHub: verificação a cada push e `.exe` portátil a cada versão (T2.2).
- Comando "Sobre o fx (versão)" na paleta, com o commit do build.
- Migalhas que não cabem viram um botão "…" com o menu das pastas escondidas.
- `fx-bench memory` mede a memória parada (P8).

- Instalador por usuário (sem administrador) além do zip portátil.
- Quick Look com os visualizadores do Windows (PDF e outros) num processo isolado, código com cores, Markdown renderizado e conteúdo de ZIP.
- Renomear em lote (F2 com vários itens): buscar e substituir com regex, numeração, letras, prévia ao vivo e um Ctrl+Z para desfazer.
- Coluna Tamanho com o total de cada pasta, calculado em segundo plano; ordenar por tamanho inclui as pastas.
- Busca com filtros digitados (`tipo:imagem`, `ext:pdf`, `tamanho:>10mb`, `data:semana`), alcance disco inteiro ou pasta (Tab), colunas ordenáveis, arrastar e menu do Windows nos resultados.
- Serviço opcional `fx-indexd` para a busca no disco inteiro (ativado pela paleta, com o aviso de administrador do Windows; remove sem deixar nada).
- Testes de falha do motor de operações (disco cheio, arquivo em uso, processo morto, > 4 GB).

### Corrigido
- Uma cópia interrompida porque o fx foi encerrado à força deixava um arquivo temporário no destino; agora ele é apagado na próxima abertura.
- O fx podia fechar sozinho na abertura em certas larguras de janela (texto truncado com reticências).

## [0.1.0] — 2026-11-21

Fase 1 — MVP de navegação. Primeira versão com o pipeline; ainda é pré-beta.

### Adicionado
- Abas com histórico, barra de caminho em migalhas com autocompletar, lateral com Acesso rápido e unidades.
- Lista e grade com ícones e miniaturas do Windows, três densidades, tema claro e escuro, fundo Mica.
- Copiar, recortar, colar, renomear, nova pasta, Lixeira, desfazer e refazer, com fila, pausa e conflitos.
- Arrastar e soltar, menu de contexto completo do Windows, abrir com, abrir no Terminal.
- Ordenar por coluna, coluna Tipo, larguras ajustáveis; itens ocultos (Ctrl+H); Propriedades (Alt+Enter).
- Quick Look (Espaço), busca no disco (Ctrl+F) e paleta de comandos (Ctrl+K).
- Abas, janelas e configurações restauradas ao reabrir; leitor de tela (UI Automation).

### Desempenho (release, Ryzen 7 5700X)
- Pasta com 100 mil arquivos com o fx aberto: 10 ms (Explorador: 254 ms).
- Tecla → quadro: p95 entre 3 e 5 ms em todas as telas medidas.
