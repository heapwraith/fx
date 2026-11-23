# Changelog

Todas as mudanças relevantes do fx. Versões seguem o [versionamento semântico](https://semver.org/lang/pt-BR/): enquanto for 0.x, cada fase do projeto sobe o número do meio (Fase 2 → 0.2). O pipeline publica uma versão quando recebe a tag `vX.Y.Z` igual à versão do `Cargo.toml`; as notas do release saem da seção da versão aqui.

## [Não lançado]

Fases 3 e 4: virar o padrão, recursos avançados, instalador próprio e atualizador.

### Adicionado
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
- **Barra lateral pelo teclado (F6)** e dicas com a ação e o atalho nos botões que são só um ícone.
- Paleta centralizada e com um ícone por comando; guia "Como usar" com botão de fechar, rodapé sempre visível e teclas separadas; a busca mostra o que digitar antes da primeira letra.
- Registro de avisos e erros em `%LOCALAPPDATA%\fx\logs\fx.log` (local, até 1 MB), para diagnosticar problemas.
- Esperas da interface não ocupam mais o pool de threads do Windows; a atualização roda numa thread própria.

### Corrigido
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
