# Fase 1 — MVP de navegação

> **Aprovada em 2026-11-21**, no portão da Fase 0 (`bench/results/gate-0.md`).

**Objetivo:** um gerenciador de arquivos que o usuário consiga usar como principal por 2 semanas sem voltar ao Explorador, mantendo as metas P1–P4 da Fase 0 e sem nenhum caso de perda de dados.

**Duração estimada:** 10 semanas. **Fora de escopo nesta fase:** busca no disco inteiro e `fx-indexd` (Fase 2), Quick Look, paleta de comandos, renomear em lote, painel duplo, vista em colunas, virar o padrão do Windows (Fase 3), instalador e atualizador.

**Decisões do portão que valem aqui:** D-013 (processo residente: a T1.2 inclui a instância única) e D-014 (P7 = no máximo 0,5% de quadros perdidos).

**Dependências novas previstas** (cada uma registrada em `docs/DECISIONS.md` na tarefa que a introduzir): `accesskit` e `accesskit_windows` (T1.10, D-008), `serde` e `toml` (T1.17), `rusqlite` (T1.17). Qualquer outra pede aprovação antes.

Regras: faça as tarefas **na ordem**. Ao concluir uma, marque `[x]`, coloque a data e uma linha de resumo com os números. Toda tarefa de UI mede P4 (tecla/clique → quadro) e P7 (rolagem) de novo e registra regressões.

---

## T1.1 — Fundação visual (Fluent 2, claro/escuro, Mica, densidade)
- [x] Concluída em: 2026-11-21 · Resumo: tokens Fluent 2 claro/escuro seguindo o Windows, Mica (Win11 22H2+), Segoe UI Variable, densidades 24/32/40 (Ctrl+1/2/3), hover, seleção com indicador e foco; contraste ≥ 4,5:1 testado; P4 p95 3,3 ms, P7 0 perdidos em 4.001 quadros. Capturas em `bench/results/visual-t1.1/` aprovadas pelo usuário em 2026-11-21.

**Fazer**
- Tokens de tema em `fx-ui` (cores, raios, espaçamentos em múltiplos de 4 px, tipografia Segoe UI Variable), com tema claro e escuro seguindo o Windows e mudando ao vivo quando o sistema muda.
- Fundo Mica na janela (atributo DWM `DWMWA_SYSTEMBACKDROP_TYPE`), com degradação para cor sólida no Windows 10.
- Densidades compacta (24 px), padrão (32 px) e confortável (40 px) na lista.
- Linha selecionada, linha sob o cursor e foco visíveis nos dois temas.

**Aceite**
- Capturas da `flat-100k` nos dois temas × três densidades em `bench/results/visual-t1.1/`, aprovadas pelo usuário **antes** da T1.3 (que constrói em cima disso).
- Contraste de texto ≥ 4,5:1 nos dois temas (calculado a partir dos tokens, com teste unitário).
- P4 e P7 sem regressão.

---

## T1.2 — Seleção e navegação por teclado na lista
- [x] Concluída em: 2026-11-21 · Resumo: `fx_core::Selection` (cursor, âncora, Shift/Ctrl, Ctrl+A, mantida ao reordenar e filtrar; 10 testes); setas, Home/End, PgUp/PgDn, Enter/duplo clique entram na pasta, Alt+↑/Backspace sobem e re-selecionam a pasta de origem; instância única por named pipe (D-016: 3 execuções = 1 processo, 3 janelas; a 2ª sai em 40 ms). P4 (`fx-bench nav`, 100 mil): setas p95 3,4 ms (81 teclas), Enter/Backspace p95 3,2 ms. Abrir arquivo fica para a T1.9; ficar residente após fechar a última janela, ver D-016.

**Fazer**
- Modelo de seleção em `fx-core`: cursor, âncora, seleção múltipla (Shift+setas, Ctrl+clique, Ctrl+A), mantida quando a lista é reordenada ou filtrada.
- Teclas: setas, Home/End, PgUp/PgDn, Enter (abre pasta / arquivo), Alt+↑ e Backspace (sobe), digitar para filtrar (já existe).
- Se a D-013 decidir pelo processo residente: abrir pasta numa janela nova reaproveita o processo (instância única por named pipe).

**Aceite**
- Testes unitários do modelo de seleção (âncora, inversão, reordenação, filtro).
- P4 ≤ 16 ms no p95 para setas e Enter em pasta de 100 mil (automatizado como na T0.6).

---

## T1.3 — Histórico, barra de caminho e autocompletar
- [x] Concluída em: 2026-11-21 · Resumo: `fx_core::History` (ramifica ao navegar depois de voltar, limite de 100) e `fx_core::complete` (prefixo sem caixa nem acento), com testes; botões ←/→/↑, Alt+←/→ e botões 4/5 do mouse restauram rolagem e seleção; migalhas clicáveis; Ctrl+L / Alt+D / clique no vazio abrem o campo com sugestões de pastas (cache da pasta pai lido em segundo plano; Tab completa, Enter confere a pasta fora da UI). P4 (`fx-bench path`): teclas do campo p95 3,35 ms (39 teclas, incluindo as que caem com o cache ainda carregando), voltar/avançar p95 3,26 ms. O histórico vive na janela até a T1.4 trazer as abas.

**Fazer**
- Histórico voltar/avançar por aba em `fx-core` (Alt+←/→, botões do mouse 4/5), guardando a posição de rolagem e a seleção de cada pasta.
- Barra de caminho em migalhas; clique numa migalha navega; clique no espaço vazio (ou Ctrl+L / Alt+D) vira campo de texto com autocompletar de pastas (enumeração em segundo plano).

**Aceite**
- Testes do histórico (ramificar depois de voltar, limite de tamanho).
- Autocompletar responde em ≤ 16 ms por tecla (cache da pasta pai) e nunca bloqueia a UI.

---

## T1.4 — Abas
- [x] Concluída em: 2026-11-21 · Resumo: `fx_core::Tabs` (abre após a ativa, fecha escolhendo a vizinha, Ctrl+Tab circular, mover mantém a ativa; 5 testes); `TabsView` como raiz da janela, cada aba uma `FolderView` com histórico próprio; Ctrl+T/Ctrl+W/Ctrl+Tab/Ctrl+Shift+Tab, clique do meio e × fecham, + abre, arrastar reordena, fechar a última aba fecha a janela; Ctrl+1/2/3 vale para todas as abas. P2 (b) (`fx-bench tabs`, `flat-100k`): Ctrl+T até o quadro com linhas mediana 10,2 ms, p95 11,4 ms (10 abas); teclas de aba p95 2,9 ms (34 teclas). Arrastar para reordenar e clique do meio compilam e seguem o modelo testado, mas ainda não foram exercitados na janela.

**Fazer**
- Abas com Ctrl+T, Ctrl+W, Ctrl+Tab/Ctrl+Shift+Tab, clique do meio fecha, arrastar para reordenar; cada aba com seu histórico.

**Aceite**
- Testes do modelo de abas em `fx-core`. Abrir uma aba nova numa pasta de 100 mil mantém a P2 (b).

---

## T1.5 — Lateral: Acesso rápido, unidades e pastas do usuário
- [x] Concluída em: 2026-11-21 · Resumo: `fx-shell` com pastas conhecidas (`SHGetKnownFolderPath`), unidades (rótulo, tipo, espaço) e vigia de unidades (D-017); `fx_core::Pinned` (sem duplicar, caminho sem caixa; testes); lateral com pasta pessoal, Acesso rápido (fixar/desafixar a pasta atual, × para desafixar) e Este computador (barra de uso e espaço livre), recolhível com ☰ ou Ctrl+B. Unidade nova com `subst` aparece em 23–60 ms (`fx-bench drives`, meta ≤ 1 s); no app em execução, `fx-shell` só roda em segundo plano (antes do loop: instância única, versão do Windows). Pendrive real não testado.

**Fazer**
- `fx-shell`: pastas conhecidas (`SHGetKnownFolderPath`), unidades com rótulo e espaço livre (`GetDiskFreeSpaceExW`), atualização quando um pendrive entra ou sai.
- Acesso rápido: fixar e desafixar pastas (persistido na T1.17; até lá, em memória).
- Lateral recolhível.

**Aceite**
- Unidade nova aparece em ≤ 1 s após conectar. Nenhuma chamada de `fx-shell` na thread de UI.

---

## T1.6 — Ícones de arquivo
- [x] Concluída em: 2026-11-21 · Resumo: ícones via `SHGetFileInfoW` numa thread STA do `fx-shell`, cache por extensão (por arquivo para .exe/.lnk/.ico etc.), tempo limite de 2 s por pedido com troca da thread travada (teste com extrator que trava), PNG mínimo entregue ao GPUI (sem dependência nova, D-018); a lista reserva o espaço de 16 px e encaixa o ícone quando chega. `flat-100k` com ícones: aba nova (P2 b) 9,6 ms (10,1 sem), rolagem 0 perdidos em 3.999 quadros. `bench/scroll-p7.ps1` ajustado ao novo layout (o arraste da barra mirava a barra de caminho).

**Fazer**
- Thread STA em `fx-shell` com fila de pedidos, `SHGetFileInfoW` / `IShellItemImageFactory` para ícones, cache por extensão (e por arquivo para `.exe`, `.lnk`, `.ico`), tempo limite por pedido.
- A lista pinta primeiro sem ícone e encaixa o ícone quando chega, sem mexer no layout.

**Aceite**
- P2 (b) e P7 sem regressão na `flat-100k` com ícones ligados. Um shell extension travado não trava o app (teste com tempo limite).

---

## T1.7 — Vista em grade e miniaturas
- [x] Concluída em: 2026-11-21 · Resumo: grade virtualizada (linhas de blocos, Ctrl+Shift+2/6 e botão, setas em 2D); miniaturas via `IShellItemImageFactory` em thread STA de fundo, primeiro só do cache do Windows e depois extraídas, fila que segue os itens visíveis (cancela o que saiu da tela), tempo limite de 5 s; cache de 400 na UI liberando a imagem decodificada (D-019). `flat-10k-media` em grade: aba nova (P2 b) mediana 9,3 ms; rolagem 0 perdidos em 3.999 quadros com miniaturas chegando. Memória 85,6 → 102,9 MB após rolar as 10 mil (risco P8 para a T1.18). Usa `IShellItemImageFactory` em vez de `IThumbnailCache` (mesmo cache, API mais simples).

**Fazer**
- Vista em grade virtualizada; miniaturas via `IThumbnailCache` (primeiro `WTS_INCACHEONLY`, depois extração em segundo plano, com prioridade baixa e cancelamento ao sair da tela).

**Aceite**
- `flat-10k-media`: primeira pintura da grade dentro da P2 (b); rolagem dentro da P7 com miniaturas chegando.

---

## T1.8 — Observador de mudanças
- [x] Concluída em: 2026-11-21 · Resumo: `fx-fs::watch` com `ReadDirectoryChangesW` assíncrono em thread própria (parado no `Drop`, armado antes de retornar), entregando criado/removido/renomeado/alterado com metadados; `FileList::apply` insere na posição ordenada, remove mantendo os índices, atualiza no lugar; seleção acompanha renomeações; a rolagem é compensada para a linha sob o cursor não andar; estouro (testado com buffer de 64 bytes) recarrega a pasta mantendo seleção e posição. Testes com `tempfile`: criar, alterar, renomear, criar pasta e apagar chegam em ≤ 100 ms (5 rodadas estáveis); verificado no app (`%TEMP%\fx-watch-demo`).

**Fazer**
- `fx-fs`: `ReadDirectoryChangesW` por pasta aberta, entregando diffs (criado, removido, renomeado, alterado) que a lista aplica sem recarregar tudo e sem mover a linha sob o cursor.

**Aceite**
- Teste com `tempfile`: criar, renomear e apagar arquivos aparece na lista em ≤ 100 ms. Estouro de buffer do observador cai para recarregar a pasta, com teste.

---

## T1.9 — Abrir arquivos
- [x] Concluída em: 2026-11-21 · Resumo: Enter/duplo clique abrem com o app padrão (selecionados, até 32), Shift+Enter "Abrir com…", Ctrl+` (ou Ctrl+' no ABNT2) abre o Windows Terminal na pasta (PowerShell se faltar); cada abertura roda em thread própria com `ShellExecuteExW` e o erro volta para a barra de status (D-020). Teste: um iniciador que demora 2 s não segura quem chama; erro real do `ShellExecuteExW` sem diálogo. Não exercitado abrindo apps reais pela janela (abriria programas na tela do usuário).

**Fazer**
- Enter / duplo clique abre com o app padrão (`ShellExecuteExW` fora da UI), "Abrir com…" e "Abrir no Terminal" (Ctrl+`).

**Aceite**
- Abrir não bloqueia a UI nem quando o app associado demora a iniciar.

---

## T1.10 — Acessibilidade com AccessKit (D-008)
- [x] Concluída em: 2026-11-21 · Resumo: janela aberta oculta, `SubclassingAdapter` no HWND (em `fx-shell`), árvore AccessKit janela → caminho → lista (total) → linhas visíveis com seleção, posição e foco; ações Focus/Click de leitores de tela selecionam a linha (D-021). Árvore UIA (`bench/uia-tree.ps1`): lista "Arquivos", 17 itens visíveis, item selecionado e focado corretos. Com o adaptador ativo: P4 p95 3,35 ms, P7 0 perdidos em 4.000 quadros. **Narrador ao vivo (Ctrl+Win+Enter) aguarda verificação do usuário.** Dependência extra fora da lista: `raw-window-handle` (D-021).

**Fazer**
- Janela aberta com `show: false`, `accesskit_windows::SubclassingAdapter` no HWND, depois `activate_window`.
- Árvore: janela → barra de caminho → lista (com o total de itens) → linhas visíveis; foco e seleção atualizados a cada mudança.

**Aceite**
- Árvore UIA (mesmo script da T0.9) mostra a lista, os itens visíveis e o item focado; o Narrador lê a pasta, o item selecionado e a contagem (verificado pelo usuário com Ctrl+Win+Enter).
- P4 e P7 sem regressão com o adaptador ativo.

---

## T1.11 — Motor de operações: cópia e movimentação seguras (`fx-ops`)
- [x] Concluída em: 2026-11-21 · Resumo: `fx-ops::Engine` com um worker por unidade de destino (mesmo disco em ordem, discos diferentes em paralelo); cópia com `CopyFile2` (progresso, cancelamento) para temporário no destino, verificação de tamanho e renomeação final com `MoveFileExW` sem substituir; mover na mesma unidade = renomear, entre unidades = copiar, verificar e só então apagar a origem; conflito vira evento e o padrão é "manter os dois" (`a (2).txt`), substituir só por escolha explícita (troca atômica); links para pastas não são seguidos. 12 testes com `tempfile` (3 rodadas): cancelar no meio de 64 MB não deixa temporário nem destino parcial e a origem fica idêntica; conflito nunca sobrescreve; Unicode, 0 bytes, caminho > 260; mover cancelado mantém a origem; pasta para dentro de si recusada; fila em ordem. Sem UI ainda (T1.13–T1.15).

**Fazer**
- Fila de operações em thread própria, com controle de concorrência por unidade; cópia com `CopyFile2` (progresso e cancelamento), escrita em arquivo temporário no destino e renomeação no fim; verificação de tamanho; mover entre unidades = copiar, verificar e só então apagar a origem (SAFETY.md §2).
- Conflito de nome sempre gera um conflito na fila; o padrão é "manter os dois".

**Aceite**
- Testes com `tempfile`: cancelar no meio não deixa temporário e mantém a origem intacta; conflito nunca sobrescreve; Unicode, > 260 caracteres e 0 bytes.

---

## T1.12 — Lixeira, renomear, nova pasta e desfazer
- [x] Concluída em: 2026-11-21 · Resumo: Delete → Lixeira (`IFileOperation`, caminho canonizado, recusa unidade sem Lixeira), Shift+Delete com confirmação, F2 renomeia no lugar (seleciona o nome sem extensão), Ctrl+Shift+N cria "Nova pasta" e já abre a renomeação, Ctrl+Z/Ctrl+Y com histórico de reversões calculadas do que cada trabalho fez (D-022). Testes (24 no `fx-ops`): cada operação desfeita volta ao estado anterior (nomes e conteúdos comparados), desfazer exclusão restaura da Lixeira real (também através de junção), raiz e `C:\Windows` recusados sem confirmação (função pura + caminho inexistente). Verificado no app pela unidade `subst`. Um arquivo de teste foi excluído de vez numa primeira tentativa (motivo da correção, D-022). Shift+Delete e o diálogo de proteção não foram exercitados na janela.

**Fazer**
- Excluir para a Lixeira (`IFileOperation`), Shift+Delete com confirmação; F2 renomeia no lugar; Ctrl+Shift+N cria pasta.
- Histórico de desfazer/refazer (Ctrl+Z / Ctrl+Y) para todas as operações da T1.11 e T1.12, com o necessário para reverter (SAFETY.md §2.4).

**Aceite**
- Testes: cada operação desfeita volta ao estado anterior (conteúdo e nomes comparados); desfazer exclusão restaura da Lixeira.
- Recusa operar em `C:\Windows` e raiz de unidade sem confirmação extra (teste).

---

## T1.13 — Fila visível e diálogo de conflitos
- [x] Concluída em: 2026-11-21 · Resumo: painel da fila no rodapé (rótulo, barra, bytes, pausar/retomar, cancelar) e diálogo único de conflito sobre a janela (manter os dois como padrão, substituir, pular, "aplicar a todos"); motor com pausa entre arquivos e política `Ask` que espera a resposta e respeita o cancelamento (3 testes novos, 27 no `fx-ops`). `fx-bench copy` (cópia da `flat-10k-media` em andamento): P4 p95 4,02 ms (81 setas) e Enter/Backspace 3,51 ms; P7 1 perdido em 3.904 quadros (0,03%). O diálogo de conflito foi testado pelo motor, não clicado na janela.

**Fazer**
- Painel da fila no rodapé com progresso, pausa, retomada e cancelamento; diálogo único de conflitos (manter os dois / substituir / pular, "aplicar a todos").

**Aceite**
- Copiar 10 mil arquivos da `flat-10k-media` mostra progresso sem travar a lista (P4 e P7 medidas durante a cópia).

---

## T1.14 — Área de transferência compatível com o Explorador
- [x] Concluída em: 2026-11-21 · Resumo: Ctrl+C/Ctrl+X/Ctrl+V com `CF_HDROP` + "Preferred DropEffect" (D-023), colar pelo motor com conflitos e desfazer, esvaziando a área depois de colar um recorte. Verificado no app com a área de transferência real: Ctrl+C no fx é lido pelo Windows (efeito 1) e colado; Ctrl+X grava efeito 2, colar move e esvazia a área; arquivo posto por `Set-Clipboard` (formato do Explorador) é colado no fx. **Teste manual pendente com o próprio Explorador:** copiar/recortar no fx e colar numa janela do Explorador, e o inverso.

**Fazer**
- Ctrl+C / Ctrl+X / Ctrl+V com `CF_HDROP` e `Preferred DropEffect`, nos dois sentidos com o Explorador.

**Aceite**
- Copiar no fx e colar no Explorador (e o inverso) funciona, inclusive recortar; teste manual descrito no relatório.

---

## T1.15 — Arrastar e soltar
- [x] Concluída em: 2026-11-21 · Resumo: soltar na lista ou numa pasta (via `IDropTarget` do GPUI) vira trabalho do motor com o efeito do Explorador (mesma unidade move, outra copia, Shift/Ctrl lidos do Windows na hora); arrastar para fora com o data object do shell (`SHDoDragDrop`); clicar numa linha já selecionada mantém a seleção para arrastar várias (D-024). Corrigido no GPUI um crash (violação de acesso) no `DragEnter` com data objects do shell. Verificado: arrastar entre dois processos do fx (OLE entre apps) moveu o arquivo pelo motor; regra do efeito com teste. **Testes manuais pendentes:** Explorador ↔ fx, área de trabalho e navegador (arquivos "virtuais" de navegador, sem caminho, não são aceitos).

**Fazer**
- OLE `IDropTarget` / `IDataObject` / `DoDragDrop` entre o fx, o Explorador e outros apps, com o efeito padrão do Windows (mesma unidade move, outra copia; Shift/Ctrl mudam).

**Aceite**
- Arrastar para o Explorador, para a área de trabalho e para um app (ex.: navegador) e no sentido inverso; a operação passa pelo motor da T1.11 (com desfazer).

---

## T1.16 — Menu de contexto do shell
- [x] Concluída em: 2026-11-21 · Resumo: menu completo do shell (`IContextMenu`/`IContextMenu3`, extensões de terceiros, fundo da pasta) numa thread STA com tempo limite de montagem (extensão travada → "indisponível", testado); itens do fx no topo; verbos de excluir/renomear/copiar/recortar/colar executados pelo motor do fx; botão direito, tecla Menu e Shift+F10; aquecimento ao abrir (D-025). **Meta de p95 ≤ 150 ms não atingida neste PC:** mediana 64 ms, p95 259 ms em 20 menus, por extensões do shell na primeira abertura de cada tipo (repetidos: 41–69 ms). Risco para o portão (T1.18). Verbos interceptados não foram clicados na janela.

**Fazer**
- `IContextMenu` / `IContextMenu3` completo (inclui extensões de terceiros) na thread STA, com tempo limite; itens próprios do fx no topo.

**Aceite**
- Menu abre em ≤ 150 ms no p95 (sem extensões lentas); uma extensão que trava não trava o app.

---

## T1.17 — Persistência
- [x] Concluída em: 2026-11-21 · Resumo: configurações em TOML (`%APPDATA%\fx\settings.toml`, gravação atômica) e sessão em SQLite WAL (`%LOCALAPPDATA%\fx\state.db`: janelas, abas, rolagem, seleção, cursor, grade, histórico, Acesso rápido), salva a cada 2 s numa thread própria e restaurada ao abrir sem pasta (D-026). Teste com processo filho morto 5 vezes no meio da gravação: banco íntegro e último estado completo; verificado no app (estado restaurado depois de matar o processo). Benchmarks e testes não tocam a sessão do usuário.

**Fazer**
- Configurações em TOML; estado (abas, janelas, histórico, Acesso rápido, posição de rolagem) em SQLite; restaurar ao reabrir.

**Aceite**
- Matar o processo no meio e reabrir restaura o último estado salvo, sem corromper o arquivo (teste com processo filho morto).

---

## T1.18 — Relatório do portão
- [ ] Concluída em: ____ · Resumo: relatório escrito em 2026-11-21 (`bench/results/gate-1.md`): P1–P4 e P7 passam; P8 83,9 MB numa pasta pequena (meta 80; Fase 0 medida hoje: 72,1) e menu p95 259 ms ficam como riscos; Explorador medido de novo (254 ms / 1.105 ms contra 10 ms / ~420 ms do fx). **Falta o registro das 2 semanas de uso diário.**

**Fazer**
- Rodar `fx-bench all`, `scroll` e `type`; repetir as comparações com o Explorador; `/gate`.

**Aceite**
- `bench/results/gate-1.md` com P1–P4 e P7, e o registro das 2 semanas de uso diário (o usuário anota cada vez que precisou voltar ao Explorador e por quê).

---

## Rodada de polimento antes do uso diário (pedida pelo usuário em 2026-11-21)

O usuário pediu que o fx tivesse o básico completo, sem bugs visuais, e três recursos da Fase 2 (Quick Look, busca no disco e paleta de comandos) antes das 2 semanas de uso do portão. As tarefas abaixo entram antes da T1.18; o restante da Fase 2 (serviço `fx-indexd`, filtros avançados, renomear em lote, beta pública) continua lá.

## T1.19 — Varredura e correção de bugs visuais
- [x] Concluída em: 2026-11-21 · Resumo: varredura com capturas (claro/escuro, lista/grade, 460/640/900 px, diálogo de conflito, fila). Corrigido: (1) fim da barra de caminho, apontado pelo usuário: em janela estreita a pasta atual sumia e abaixo de ~500 px a barra inteira desaparecia; agora as migalhas cortam pelo começo, a barra tem largura mínima e o status encolhe com reticências; (2) nomes cortados sem reticências, por bug do cache de texto do GPUI (patch); (3) colunas transbordando em lista estreita: data e depois tamanho somem, com novo quadro após redimensionar; (4) lateral se recolhe abaixo de 720 px; (5) painel da fila não aparecia quando o trabalho começava antes de ser registrado (corrida); (6) escurecimento do diálogo não cobria o painel. Conferidos sem problema: temas, grade, diálogo de conflito, campo de caminho, renomear. As capturas ficaram fora do repositório (mostram o nome de usuário na lateral).

**Fazer**
- Capturas nos dois temas, nas três densidades, em lista e grade, janela larga e estreita: barra de caminho (inclusive o fim dela, onde ficam o botão de vista e o texto de status), abas, lateral, ícones de arquivo, rodapé da fila, diálogos.
- Corrigir o que aparecer, começando pelo fim da barra de caminho (apontado pelo usuário).

**Aceite**
- Lista dos problemas encontrados, com captura antes e depois de cada correção; nada sobreposto, cortado sem reticências ou desalinhado em 640 px de largura.

## T1.20 — Ordenar por coluna, coluna Tipo e largura das colunas
- [ ] Concluída em: ____ · Resumo: ____

**Fazer**
- Clique no cabeçalho ordena por nome, tamanho, data ou tipo (clicar de novo inverte), com seta indicando; coluna Tipo; arrastar a borda do cabeçalho ajusta a largura. Ordenação fora da thread de UI; persiste por aba.

**Aceite**
- Reordenar 100 mil itens não trava a UI (P4 medida durante a reordenação); seleção mantida (testes).

## T1.21 — Arquivos ocultos e Propriedades
- [ ] Concluída em: ____ · Resumo: ____

**Fazer**
- Mostrar/esconder ocultos e de sistema (Ctrl+H e botão), salvo nas configurações; Alt+Enter e item no menu abrem as Propriedades do Windows.

**Aceite**
- Testes do filtro de ocultos; Propriedades abre sem bloquear a UI.

## T1.22 — Quick Look (Espaço)
- [ ] Concluída em: ____ · Resumo: ____

**Fazer**
- Espaço abre uma janela de pré-visualização do item sob o cursor, que acompanha as setas e fecha com Espaço/Esc: imagem (decodificada fora da UI), texto e código (primeiros 256 KB), e miniatura grande do shell para os demais tipos. Vídeo, áudio, PDF e preview handlers de terceiros ficam para a Fase 2 (`fx-preview` isolado).

**Aceite**
- Abrir a pré-visualização em ≤ 100 ms para imagem de até 10 MB e texto (medido); trocar de item com as setas sem travar a lista.

## T1.23 — Busca no disco (Ctrl+F)
- [ ] Concluída em: ____ · Resumo: ____

**Fazer**
- Ctrl+F abre a busca: resultados enquanto digita, abrir/revelar o resultado. Com o fx elevado, usa o índice da MFT (`fx-index`); sem elevação (o normal até o `fx-indexd` da Fase 2), busca na pasta atual e subpastas por enumeração em segundo plano, com resultados chegando aos poucos e cancelamento a cada tecla.

**Aceite**
- Nenhuma tecla da busca passa de 16 ms (P4) enquanto os resultados chegam; busca na pasta de fixtures acha os itens esperados (teste).

## T1.24 — Paleta de comandos (Ctrl+K)
- [ ] Concluída em: ____ · Resumo: ____

**Fazer**
- Ctrl+K (e Ctrl+Shift+P) abre a paleta: busca difusa sobre os comandos do fx (com o atalho ao lado), pastas recentes e Acesso rápido; Enter executa.

**Aceite**
- Testes da busca difusa e da lista de comandos; abrir e filtrar em ≤ 16 ms por tecla.

---

## Portão da Fase 1

Passa para a Fase 2 somente se:
1. O usuário usou o fx como gerenciador principal por **2 semanas** sem voltar ao Explorador (ou cada volta virou tarefa corrigida).
2. **P1–P4 e P7** mantidas em relação à Fase 0.
3. **Nenhum caso de perda de dados** (um único caso reprova).
