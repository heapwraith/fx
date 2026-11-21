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
- [ ] Concluída em: ____ · Resumo: ____

**Fazer**
- Abas com Ctrl+T, Ctrl+W, Ctrl+Tab/Ctrl+Shift+Tab, clique do meio fecha, arrastar para reordenar; cada aba com seu histórico.

**Aceite**
- Testes do modelo de abas em `fx-core`. Abrir uma aba nova numa pasta de 100 mil mantém a P2 (b).

---

## T1.5 — Lateral: Acesso rápido, unidades e pastas do usuário
- [ ] Concluída em: ____ · Resumo: ____

**Fazer**
- `fx-shell`: pastas conhecidas (`SHGetKnownFolderPath`), unidades com rótulo e espaço livre (`GetDiskFreeSpaceExW`), atualização quando um pendrive entra ou sai.
- Acesso rápido: fixar e desafixar pastas (persistido na T1.17; até lá, em memória).
- Lateral recolhível.

**Aceite**
- Unidade nova aparece em ≤ 1 s após conectar. Nenhuma chamada de `fx-shell` na thread de UI.

---

## T1.6 — Ícones de arquivo
- [ ] Concluída em: ____ · Resumo: ____

**Fazer**
- Thread STA em `fx-shell` com fila de pedidos, `SHGetFileInfoW` / `IShellItemImageFactory` para ícones, cache por extensão (e por arquivo para `.exe`, `.lnk`, `.ico`), tempo limite por pedido.
- A lista pinta primeiro sem ícone e encaixa o ícone quando chega, sem mexer no layout.

**Aceite**
- P2 (b) e P7 sem regressão na `flat-100k` com ícones ligados. Um shell extension travado não trava o app (teste com tempo limite).

---

## T1.7 — Vista em grade e miniaturas
- [ ] Concluída em: ____ · Resumo: ____

**Fazer**
- Vista em grade virtualizada; miniaturas via `IThumbnailCache` (primeiro `WTS_INCACHEONLY`, depois extração em segundo plano, com prioridade baixa e cancelamento ao sair da tela).

**Aceite**
- `flat-10k-media`: primeira pintura da grade dentro da P2 (b); rolagem dentro da P7 com miniaturas chegando.

---

## T1.8 — Observador de mudanças
- [ ] Concluída em: ____ · Resumo: ____

**Fazer**
- `fx-fs`: `ReadDirectoryChangesW` por pasta aberta, entregando diffs (criado, removido, renomeado, alterado) que a lista aplica sem recarregar tudo e sem mover a linha sob o cursor.

**Aceite**
- Teste com `tempfile`: criar, renomear e apagar arquivos aparece na lista em ≤ 100 ms. Estouro de buffer do observador cai para recarregar a pasta, com teste.

---

## T1.9 — Abrir arquivos
- [ ] Concluída em: ____ · Resumo: ____

**Fazer**
- Enter / duplo clique abre com o app padrão (`ShellExecuteExW` fora da UI), "Abrir com…" e "Abrir no Terminal" (Ctrl+`).

**Aceite**
- Abrir não bloqueia a UI nem quando o app associado demora a iniciar.

---

## T1.10 — Acessibilidade com AccessKit (D-008)
- [ ] Concluída em: ____ · Resumo: ____

**Fazer**
- Janela aberta com `show: false`, `accesskit_windows::SubclassingAdapter` no HWND, depois `activate_window`.
- Árvore: janela → barra de caminho → lista (com o total de itens) → linhas visíveis; foco e seleção atualizados a cada mudança.

**Aceite**
- Árvore UIA (mesmo script da T0.9) mostra a lista, os itens visíveis e o item focado; o Narrador lê a pasta, o item selecionado e a contagem (verificado pelo usuário com Ctrl+Win+Enter).
- P4 e P7 sem regressão com o adaptador ativo.

---

## T1.11 — Motor de operações: cópia e movimentação seguras (`fx-ops`)
- [ ] Concluída em: ____ · Resumo: ____

**Fazer**
- Fila de operações em thread própria, com controle de concorrência por unidade; cópia com `CopyFile2` (progresso e cancelamento), escrita em arquivo temporário no destino e renomeação no fim; verificação de tamanho; mover entre unidades = copiar, verificar e só então apagar a origem (SAFETY.md §2).
- Conflito de nome sempre gera um conflito na fila; o padrão é "manter os dois".

**Aceite**
- Testes com `tempfile`: cancelar no meio não deixa temporário e mantém a origem intacta; conflito nunca sobrescreve; Unicode, > 260 caracteres e 0 bytes.

---

## T1.12 — Lixeira, renomear, nova pasta e desfazer
- [ ] Concluída em: ____ · Resumo: ____

**Fazer**
- Excluir para a Lixeira (`IFileOperation`), Shift+Delete com confirmação; F2 renomeia no lugar; Ctrl+Shift+N cria pasta.
- Histórico de desfazer/refazer (Ctrl+Z / Ctrl+Y) para todas as operações da T1.11 e T1.12, com o necessário para reverter (SAFETY.md §2.4).

**Aceite**
- Testes: cada operação desfeita volta ao estado anterior (conteúdo e nomes comparados); desfazer exclusão restaura da Lixeira.
- Recusa operar em `C:\Windows` e raiz de unidade sem confirmação extra (teste).

---

## T1.13 — Fila visível e diálogo de conflitos
- [ ] Concluída em: ____ · Resumo: ____

**Fazer**
- Painel da fila no rodapé com progresso, pausa, retomada e cancelamento; diálogo único de conflitos (manter os dois / substituir / pular, "aplicar a todos").

**Aceite**
- Copiar 10 mil arquivos da `flat-10k-media` mostra progresso sem travar a lista (P4 e P7 medidas durante a cópia).

---

## T1.14 — Área de transferência compatível com o Explorador
- [ ] Concluída em: ____ · Resumo: ____

**Fazer**
- Ctrl+C / Ctrl+X / Ctrl+V com `CF_HDROP` e `Preferred DropEffect`, nos dois sentidos com o Explorador.

**Aceite**
- Copiar no fx e colar no Explorador (e o inverso) funciona, inclusive recortar; teste manual descrito no relatório.

---

## T1.15 — Arrastar e soltar
- [ ] Concluída em: ____ · Resumo: ____

**Fazer**
- OLE `IDropTarget` / `IDataObject` / `DoDragDrop` entre o fx, o Explorador e outros apps, com o efeito padrão do Windows (mesma unidade move, outra copia; Shift/Ctrl mudam).

**Aceite**
- Arrastar para o Explorador, para a área de trabalho e para um app (ex.: navegador) e no sentido inverso; a operação passa pelo motor da T1.11 (com desfazer).

---

## T1.16 — Menu de contexto do shell
- [ ] Concluída em: ____ · Resumo: ____

**Fazer**
- `IContextMenu` / `IContextMenu3` completo (inclui extensões de terceiros) na thread STA, com tempo limite; itens próprios do fx no topo.

**Aceite**
- Menu abre em ≤ 150 ms no p95 (sem extensões lentas); uma extensão que trava não trava o app.

---

## T1.17 — Persistência
- [ ] Concluída em: ____ · Resumo: ____

**Fazer**
- Configurações em TOML; estado (abas, janelas, histórico, Acesso rápido, posição de rolagem) em SQLite; restaurar ao reabrir.

**Aceite**
- Matar o processo no meio e reabrir restaura o último estado salvo, sem corromper o arquivo (teste com processo filho morto).

---

## T1.18 — Relatório do portão
- [ ] Concluída em: ____ · Resumo: ____

**Fazer**
- Rodar `fx-bench all`, `scroll` e `type`; repetir as comparações com o Explorador; `/gate`.

**Aceite**
- `bench/results/gate-1.md` com P1–P4 e P7, e o registro das 2 semanas de uso diário (o usuário anota cada vez que precisou voltar ao Explorador e por quê).

---

## Portão da Fase 1

Passa para a Fase 2 somente se:
1. O usuário usou o fx como gerenciador principal por **2 semanas** sem voltar ao Explorador (ou cada volta virou tarefa corrigida).
2. **P1–P4 e P7** mantidas em relação à Fase 0.
3. **Nenhum caso de perda de dados** (um único caso reprova).
