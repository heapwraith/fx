# Fase 6 — Superioridade

> Registrada em 2026-11-22, a pedido do usuário, a partir da análise de funcionalidades novas: o que torna o fx **superior** aos concorrentes, não só igual. **Ainda não iniciada:** vem depois da 1.0 (Fase 5), quando o usuário pedir. A ordem abaixo é a recomendada; o usuário pode mudar.

**Objetivo:** usar as vantagens que só o fx reúne — o índice da MFT do disco inteiro em milissegundos, o diário USN (tudo o que muda no disco, em tempo real), o motor de operações seguro com desfazer e a interface nativa — para oferecer o que nenhum gerenciador de arquivos tem junto.

**Regras desta fase:**
- Todas as metas de `docs/PERFORMANCE.md` (D-064) continuam valendo; cada tarefa mede P4 e P8 e não pode passar dos limites.
- Status sempre real: nada simulado.
- Itens com risco de memória (busca no conteúdo, painel de detalhes, busca por significado) só entram se a P8 seguir dentro do limite.
- Toda tela segue a skill `fx-design` e é conferida na janela nos dois idiomas e temas.

---

## Grupo A — Diferenciais (ninguém tem; a base já existe no fx)

### T6.1 — Mapa do disco instantâneo
- [x] Concluída em: 2026-11-22 · Resumo: Ctrl+Shift+M (ou a paleta) mostra a pasta como treemap "squarified" (`fx-core/src/treemap.rs`), com os totais reais das subpastas (índice ou soma em segundo plano, com progresso real), cor por tipo; setas escolhem, Enter entra, Backspace sobe, Delete manda para a Lixeira com confirmação e desfazer; leitor de tela lê bloco, tamanho e porcentagem (4 testes novos). Sem mensagem nova no serviço: o `sizes_of` da T2.7 bastou.

Treemap de qualquer pasta ou disco mostrando o que ocupa espaço, em milissegundos; clicar navega; Delete libera (com confirmação e desfazer). **Base:** o índice já sabe o tamanho de tudo (totais de pastas em 32 ms por milhão). **Concorrentes:** WizTree/TreeSize são apps separados; o Explorador não tem.

### T6.2 — Linha do tempo do disco
- [x] Concluída em: 2026-11-22 · Resumo: o `fx-indexd` guarda as últimas 50 mil mudanças do diário USN (registros de fechamento: criado, alterado, apagado, renomeado; sem pastas de sistema e temporárias) e responde a um pedido novo (`FXQ4`; as buscas seguem `FXQ3`, compatíveis com um serviço antigo); Ctrl+Shift+L mostra 10 minutos, 1 hora ou 24 horas, atualizando sozinha a cada 3 s, com Enter mostrando o item na pasta; sem o serviço, diz como ativar (7 testes novos). Conferida a tela sem serviço; com dados reais, depende de ativar o serviço (administrador).

"O que mudou nos últimos 10 minutos, 1 hora, hoje", no disco todo: criados, alterados, apagados; achar "aquele arquivo que baixei agora há pouco". **Base:** o `fx-indexd` já segue o diário USN a cada 250 ms. **Concorrentes:** ninguém expõe o USN ao usuário.

### T6.3 — Buscas vivas
- [x] Concluída em: 2026-11-22 · Resumo: Ctrl+S no painel de busca salva a busca (texto e alcance) numa tabela `searches` do `state.db`; ela aparece na barra lateral ("Buscas", × para esquecer) e na paleta ("Busca: …"), abre o painel preenchido e, no disco inteiro com o serviço, refaz a busca sozinha a cada 10 s enquanto aberta (resultados reais); sem o serviço, busca na pasta (2 testes novos).

Uma busca vira aba ou pasta virtual que se atualiza sozinha (ex.: `tipo:pdf data:semana`, `tamanho:>1gb`), fixável na barra lateral. **Base:** filtros e serviço prontos; o USN atualiza. **Concorrentes:** o Finder tem pastas inteligentes, mais lentas; Explorador e Files não têm.

### T6.4 — Duplicados em segundos
- [x] Concluída em: 2026-11-22 · Resumo: Ctrl+Shift+D procura arquivos repetidos (4 KB ou mais) na pasta e subpastas (`fx-fs/src/duplicates.rs`): candidatos por tamanho, depois pelas pontas (64 KiB), confirmados comparando o conteúdo inteiro em blocos (nada estimado, nenhum arquivo inteiro na memória), com progresso real e cancelamento; Espaço marca, A marca as cópias, Delete manda para a Lixeira com confirmação e desfazer, e a lista reconfere a cada 3 s o que ainda existe (2 testes novos). Escopo por pasta (o disco inteiro: abrir na raiz da unidade), em vez de candidatos pelo índice, para funcionar sem o serviço.

Acha arquivos repetidos no disco todo: candidatos pelo tamanho (índice), depois SHA-256 só deles; remove com desfazer. **Base:** índice com tamanhos e SHA-256 (BCrypt). **Concorrentes:** ferramentas separadas, lentas porque leem o disco inteiro.

### T6.5 — "Ir para" que aprende
- [x] Concluída em: 2026-11-22 · Resumo: pastas abertas lembradas com a pontuação do zoxide (frequência × recência, envelhecimento, até 500) em `fx-core/src/frecency.rs`, guardadas numa tabela `visits` do `state.db` (lidas fora da interface, gravadas com a sessão); a paleta lista as pastas pelo uso real e "proj fx" acha `…\Projetos\fx` antes de qualquer comando (8 testes novos).

A paleta (Ctrl+K) ordena pastas pela frequência e recência reais de uso (como o zoxide): "proj fx" leva à pasta certa. **Base:** paleta e histórico existem. **Concorrentes:** o Explorador só tem "recentes".

### T6.6 — Desfazer entre sessões
- [x] Concluída em: 2026-11-22 · Resumo: o histórico de desfazer (até 100 operações) é gravado numa tabela `undo` do `state.db` com os trabalhos codificados sem perda (`fx-ops/src/codec.rs`) e volta na abertura seguinte por baixo do histórico novo; o desfazer continua pelo motor, que nunca sobrescreve; o painel mostra a data das operações de outros dias; operações gigantes (> 512 KiB codificadas) ficam só na sessão (8 testes novos).

O histórico de operações sobrevive ao fechar o fx ("desfazer a limpeza de ontem"), com as mesmas garantias de nunca sobrescrever. **Base:** histórico e diário do `fx-ops`. **Concorrentes:** Explorador e Files perdem o desfazer ao fechar.

## Grupo B — Paridade que falta, feita do jeito fx

### T6.7 — Arquivos compactados como pastas
- [ ] Concluída em: ____ · Resumo:

Entrar em ZIP, 7z e tar como numa pasta (mesma lista e busca); extrair arrastando; compactar pela paleta. Extração segura: sem caminhos maliciosos, com conflitos e desfazer.

### T6.8 — Busca no conteúdo
- [ ] Concluída em: ____ · Resumo:

Índice de conteúdo opcional e incremental pelo USN (texto e código primeiro), com a busca por tecla de sempre. O Windows Search é lento; o Everything só indexa nomes.

### T6.9 — Painel de detalhes
- [x] Concluída em: 2026-11-22 · Resumo: Alt+Shift+P abre, ao lado da lista (que encolhe em vez de ficar coberta), as propriedades do item do cursor pelo Property System do Windows (`fx-shell/src/windows/properties.rs`): tipo, datas, dimensões, data da foto, câmera, duração, artista, autor, páginas, versão…, nomeadas e formatadas pelo Windows; lidas numa thread só quando o item muda, nada guardado com o painel fechado (1 teste novo). As colunas de metadados e o "agrupar por" (T6.10) ficam para a próxima entrega.

Lateral direita com metadados reais (dimensões, EXIF, duração, autor), lidos em segundo plano, sem custo de memória parado.

### T6.10 — Colunas de metadados e agrupar por
- [x] Concluída em: 2026-11-23 · Resumo: o menu Exibir e a paleta ganham "Agrupar por" (tipo, data de modificação, tamanho, como o Explorador; pastas sempre primeiro) e as colunas Dimensões, Duração e Data da foto; a ordem agrupada sai com a ordenação num worker e os cabeçalhos dos grupos são achados num worker a cada mudança das linhas (100 mil linhas: tipo 2,1 ms, data 1,9 ms, tamanho 0,4 ms, fora da interface), desenhados como linhas próprias na vista de detalhes e lidos pelo leitor de tela; as colunas leem o Property System numa thread só para as linhas desenhadas, sem baixar arquivos de nuvem (`fx-core/src/grouping.rs`, 4 testes novos). Conferida nas capturas.

Dimensões, duração, data da foto como colunas; agrupar por tipo, data ou tamanho, sem travar.

### T6.11 — Comparar pastas e sincronizar
- [x] Concluída em: 2026-11-22 · Resumo: "Comparar com o outro painel" (paleta, com o painel duplo aberto) lista, abaixo das duas pastas, o que só existe de um lado e os arquivos com data (± 2 s) ou tamanho diferentes (`fx-core/src/compare.rs`, listagem em `fx-fs/src/tree.rs` numa thread, até 200 mil itens por lado, avisando se cortou); Espaço marca, → copia para a direita e ← para a esquerda pelo motor de operações (conflitos perguntam, desfazer vale), e a comparação refaz em seguida (2 testes novos).

Painel duplo mostrando o que difere entre duas pastas; sincronizar pelo motor seguro, com desfazer.

### T6.12 — Status do Git
- [x] Concluída em: 2026-11-22 · Resumo: em pastas de repositórios, cada item mostra uma letra (M, A, ?, R, D, !) lida de `git status --porcelain=v2 -z` (`fx-core/src/git_status.rs`), rodado numa thread sem janela de console e com tempo limite de 3 s (`fx_shell::run_hidden`); pastas herdam o estado mais urgente do que têm dentro; relê ao abrir a pasta e quando ela muda, nunca dois ao mesmo tempo; o leitor de tela ouve o estado em palavras; sem git ou fora de repositório, nada aparece (3 testes novos). Conferida num repositório de teste.

Ícone de modificado/novo nos itens de pastas que são repositórios, lido em segundo plano.

### T6.13 — Espaços de trabalho
- [x] Concluída em: 2026-11-22 · Resumo: "Salvar abas como espaço de trabalho…" na paleta pede o nome no próprio campo e guarda as pastas das abas e a ativa numa tabela `workspaces` do `state.db` (o mesmo nome atualiza); "Espaço: Trabalho" abre numa janela nova, sem fechar nada; "Excluir espaço" aparece ao buscar; visitas e espaços lidos por uma só conexão fora da interface (4 testes novos).

Conjuntos de abas e painéis por projeto ("Trabalho", "Fotos"), trocados pela paleta.

### T6.14 — Abas entre janelas e mais divisões
- [ ] Concluída em: ____ · Resumo:

Arrastar uma aba para fora vira janela; dividir em mais de dois painéis.

## Grupo C — Tendências de 2026 (opcional e privado)

### T6.15 — Busca por significado
- [ ] Concluída em: ____ · Resumo:

"Fotos da praia de 2024", "contrato com a Ana", usando a IA local do Windows (APIs de IA dos PCs Copilot+), **sem enviar nada**; desligada por padrão e só onde o hardware permite.

### T6.16 — Ações rápidas por tipo
- [ ] Parcial em 2026-11-22: imagens prontas — "Imagens: copiar como JPG / como PNG / cópia com metade do tamanho" na paleta, para as imagens selecionadas, pelo WIC do Windows (`fx-shell/src/windows/images.rs`), numa thread, com progresso real; o original nunca é tocado e a cópia nunca substitui nada (nome temporário, renomear sem substituir, "manter os dois") (2 testes novos). Em 2026-11-23: "Imagens: renomear pela data da foto" lê a data EXIF pelo Property System numa thread (com progresso real) e renomeia para `2024-07-15 14.30.22.jpg` pelo motor, como uma operação que o Ctrl+Z desfaz; fotos do mesmo segundo ganham " (2)", nome já existente nunca é tomado, fotos sem data ficam como estão (2 testes novos, um com JPEG e EXIF reais). Falta só juntar PDFs, que exige uma biblioteca de PDF (dependência nova: aguarda aprovação, SAFETY.md).

Converter/redimensionar imagens, juntar PDFs, renomear fotos pela data EXIF, direto da seleção, pela paleta.

---

## Análise de 2026-11-22 (D-068)

Revisão das 16 tarefas depois da revisão de código (D-066) e da análise de interface (D-067): como cada uma sai com o que o fx já tem, o que o Windows oferece pronto e o que exigiria dependência nova (que precisa de aprovação, SAFETY.md).

**Regras que valem para toda tarefa desta fase** (aprendidas nas D-066 e D-067):
- Tela nova = teclado completo (Tab, setas, Enter, Esc), anel de foco só ao usar o teclado, alvos de pelo menos 24 px, e a camada de acessibilidade declarada (`a11y::Overlay` ou nós próprios), com "x de N" nas listas.
- Nada de espera bloqueante no pool do GPUI: trabalho longo em thread própria (`chan`), resultados por canal.
- Nomes e caminhos sem conversão com perda (`OsStr`); destinos montados com `fx_core::naming`.
- Conferida nas capturas (`FX_OPEN` na build de depuração) nos dois temas, idiomas e larguras, antes de dizer que está pronta.

| Tarefa | Caminho sem dependência nova | Dependência nova (pedir aprovação) | Risco principal |
|---|---|---|---|
| T6.1 Mapa do disco | Totais de pastas do índice (`fx-indexd`, mensagem nova no protocolo `FXQ4`) e treemap "squarified" desenhado em `canvas` do GPUI | — | Disco sem o serviço: cair para os totais do `dir_sizes` (mais lento, com progresso real) |
| T6.2 Linha do tempo | O `fx-indexd` já lê o diário USN; guardar as últimas N mudanças num anel e responder por intervalo | — | Memória do serviço (P9): anel limitado (ex.: 200 mil entradas) |
| T6.3 Buscas vivas | Filtros e busca existentes; buscas salvas no `settings.toml`, atualizadas pelo USN (mesma mensagem da T6.2) | — | Muitas buscas abertas: atualizar só a visível |
| T6.4 Duplicados | Candidatos por tamanho no índice; SHA-256 com BCrypt (`fx_shell::net::sha256`) em thread própria, com progresso real e cancelamento | — | Disco inteiro: limitar por tamanho mínimo e permitir escolher a pasta |
| T6.5 "Ir para" que aprende | Contagem de uso e recência por pasta no SQLite de estado; ordenação "frecency" na paleta | — | Nenhum relevante |
| T6.6 Desfazer entre sessões | Histórico do `fx-ops` serializado no SQLite; o desfazer já nunca sobrescreve | — | Arquivos mudados desde então: o trabalho reverso falha com mensagem, não sobrescreve |
| T6.7 Compactados como pastas | Navegar e extrair pelo próprio shell do Windows (pasta compactada para ZIP; 7z, RAR e tar no Windows 11 23H2+) via `fx-shell`; a listagem de ZIP já existe (`zip_list`) | Só se for preciso ler 7z/RAR no Windows 10 | Caminhos maliciosos no arquivo: extração sempre confere o destino |
| T6.8 Busca no conteúdo | Primeiro o Windows Search (já indexa conteúdo onde o usuário ativou), com aviso quando a pasta não está indexada | Índice próprio (ex.: `tantivy`) só se o do Windows não bastar | Memória (P8) e tempo de indexação |
| T6.9 + T6.10 Detalhes e colunas | Property System do Windows (`IPropertyStore`, `SHGetPropertyStoreFromParsingName`) no `fx-shell`, em thread própria e sob demanda: dimensões, duração, EXIF, autor | — | Leitores de propriedade de terceiros lentos: tempo limite, como no menu |
| T6.11 Comparar pastas | Listagens que já existem, comparação por nome, tamanho e data; sincronizar pelo `fx-ops` (com desfazer) | — | Pastas enormes: comparar em lotes, com progresso real |
| T6.12 Status do Git | `git status --porcelain=v2 -z` em thread própria quando o `git` existe; sem ele, nada aparece | — | Repositórios grandes: tempo limite e cache por pasta |
| T6.13 Espaços de trabalho | A sessão já salva janelas e abas; espaços são sessões com nome, trocadas pela paleta | — | Nenhum relevante |
| T6.14 Abas entre janelas | Janelas múltiplas já existem; arrastar a aba para fora cria a janela e move o estado | — | API de arrastar entre janelas do GPUI: protótipo antes |
| T6.15 Busca por significado | — | Windows App SDK (APIs de IA do Windows) | Só em PCs Copilot+; opcional e desligada por padrão |
| T6.16 Ações rápidas | Imagens com o WIC do Windows (converter, redimensionar, girar); renomear pela data EXIF com a T6.9 | Juntar PDFs exige uma biblioteca de PDF (ex.: `lopdf`) | Arquivos grandes: sempre gera cópias, nunca altera o original |

**Mudanças no plano:** T6.9 e T6.10 viram uma entrega só (a mesma fonte de dados). T6.2 e T6.3 compartilham a mensagem de mudanças recentes do `fx-indexd`. A ordem passa a começar pelo que não depende de nada novo e tem maior retorno por esforço.

## Ordem recomendada

Revista na D-068 (antes: mapa, linha do tempo e buscas vivas primeiro):

1. **Base rápida, sem dependência nova:** T6.5 "Ir para" que aprende, T6.13 Espaços de trabalho, T6.6 Desfazer entre sessões.
2. **Diferenciais do índice:** T6.1 Mapa do disco, T6.2 Linha do tempo e T6.3 Buscas vivas (mensagem nova do `fx-indexd`), T6.4 Duplicados.
3. **Lacunas que fazem voltar ao Explorador:** T6.9 + T6.10 Detalhes e colunas (Property System), T6.7 Compactados pelo shell.
4. **Fluxos avançados:** T6.11 Comparar pastas, T6.12 Status do Git, T6.16 Ações rápidas (imagens; PDF só com aprovação da dependência), T6.14 Abas entre janelas (protótipo antes).
5. **Opcionais com dependência nova:** T6.8 Busca no conteúdo (Windows Search antes de índice próprio), T6.15 Busca por significado.

## Portão da Fase 6

Os diferenciais do grupo A entregues e conferidos com o usuário; todas as metas de `docs/PERFORMANCE.md` dentro do limite.
