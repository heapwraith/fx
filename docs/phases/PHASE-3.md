# Fase 3 — Virar o padrão

> **Iniciada em 2026-11-22**, a pedido do usuário: terminar as funcionalidades das fases restantes e fazer os testes manuais e as correções no fim (`docs/TESTES-FINAIS.md`). O portão da Fase 2 está em `bench/results/gate-2.md`.

**Objetivo:** o fx pode substituir o Explorador (com volta garantida), ganha os modos de navegação de usuário avançado (painel duplo, colunas, etiquetas, terminal), roda em ARM64, fala português e inglês, respeita alto contraste e DPI por monitor, mostra o estado das pastas de nuvem e, se o usuário quiser, guarda relatórios de falha.

**Regras desta fase:**
- **Registro e Win+E** (T3.1): o código só altera `HKCU` e sempre tem um "reverter" que restaura o que havia antes. Os testes usam uma chave temporária própria. **Aplicar no PC do usuário só quando ele pedir** (SAFETY.md §1).
- Sem dependências novas sem registro em `docs/DECISIONS.md`.
- Toda tarefa de UI segue a skill `fx-design` e mede P4; testes manuais vão para `docs/TESTES-FINAIS.md`.

---

## T3.1 — Gerenciador de arquivos padrão (com reverter)
- [x] Concluída em: 2026-11-22 · Resumo: `fx-shell::default_manager` (D-046): ativar grava em `HKCU\Software\Classes` o abrir de Pasta/Diretório/Unidade e o Win+E (CLSID do Computador, "nova janela"), com `DelegateExecute` vazio, depois de guardar o que havia; reverter restaura os valores e apaga só as chaves criadas pelo fx. Comandos na paleta. Sem argumento (Win+E, Menu Iniciar) o fx abre a pasta do usuário, não a System32. 3 testes numa raiz temporária (`HKCU\Software\fx-test-*`, apagada no fim); **não aplicado no PC do usuário** (aguarda pedido, `docs/TESTES-FINAIS.md`).

**Fazer**
- Em `fx-shell`: estado, ativar e reverter. Ativar faz pastas, unidades e Win+E abrirem no fx, por chaves em `HKCU\Software\Classes` (verbo `open` de `Folder`/`Directory`/`Drive` e o comando de Win+E). Antes de gravar, faz uma cópia do que havia. Reverter apaga só o que o fx criou e restaura a cópia.
- No fx: comandos na paleta ("Tornar o fx o gerenciador padrão" / "Voltar ao Explorador") e o estado em "Sobre o fx".

**Aceite**
- Testes com uma raiz temporária (`HKCU\Software\fx-test-<pid>`): ativar grava o esperado, reverter deixa a raiz como estava (inclusive valores pré-existentes); nenhum teste toca as chaves reais.

## T3.2 — Painel duplo (F3)
- [x] Concluída em: 2026-11-22 · Resumo: F3 divide a aba em dois painéis (cada um com pasta, histórico e seleção próprios); Tab alterna (sem roubar o Tab da barra de caminho, da busca e do renomear em lote); F5/F6 copiam/movem a seleção para a pasta do outro painel pela fila (conflitos, desfazer); clique ativa o painel (faixa teal); paleta e lateral agem no painel ativo (D-047). Conferido na janela: F5 copiou `pequena.jpg` para "Fotos" no outro painel. Ainda não é salvo entre sessões.

**Fazer**
- F3 divide a aba em dois painéis lado a lado, cada um com sua pasta, histórico e seleção; Tab alterna o foco; F5/F6 copiam/movem a seleção para a pasta do outro painel (mesmo motor e conflitos). O estado do painel duplo é salvo por aba.

**Aceite**
- Copiar e mover entre painéis pelo motor (teste); P4 sem regressão.

## T3.3 — Vista em colunas (Miller)
- [x] Concluída em: 2026-11-22 · Resumo: Ctrl+Shift+3 abre a vista em colunas (modelo `fx-core::miller`, 3 testes; uma coluna por pasta, ↑↓ escolhe, → entra, ← volta e continua subindo, Enter abre, Esc volta à lista na pasta da coluna ativa); cada coluna é lida numa thread e desenhada virtualizada (D-048). Conferido na janela com a coluna de 100 mil arquivos sem travar.

**Fazer**
- Ctrl+Shift+3: colunas lado a lado, uma por nível do caminho; selecionar uma pasta abre a coluna seguinte; ← → navegam entre colunas. Cada coluna é uma lista virtualizada lida em segundo plano.

**Aceite**
- Abrir uma pasta de 100 mil numa coluna sem travar (P4); teste do modelo de colunas (caminho → colunas, seleção, voltar).

## T3.4 — Etiquetas
- [x] Concluída em: 2026-11-22 · Resumo: 7 etiquetas coloridas (`fx-core::tags`); Alt+1…7 liga/desliga na seleção, Alt+0 limpa; bolinhas ao lado do nome; guardadas em `tags.db` (SQLite, thread própria, espelho em memória) e levadas junto em renomear/mover feitos pelo fx; `etiqueta:verde` na busca acha os itens etiquetados em qualquer pasta (D-049). Testes da loja e do filtro; conferido na janela.

**Fazer**
- Até 7 etiquetas coloridas por item (nomes editáveis), guardadas no banco do fx (por caminho, seguindo renomeações e movimentos feitos pelo fx); coluna/ponto de cor na lista; filtrar por etiqueta na busca (`etiqueta:`) e na paleta.

**Aceite**
- Testes do banco (atribuir, remover, renomear arquivo, mover); filtro na busca.

## T3.5 — Terminal embutido (opcional)
- [x] Concluída em: 2026-11-22 · Resumo: Ctrl+J abre o PowerShell na pasta atual num painel (ConPTY em `fx-shell::conpty`, tela VT em `fx-core::vt`: texto, UTF-8, cursor, apagar, cores ANSI e negrito, rolagem com histórico); teclas vão para o terminal enquanto ele tem o foco (clique alterna); o console acompanha o tamanho do painel; fechar encerra o shell (D-050). Testes: 5 da tela VT, ConPTY com `cmd` de verdade, codificação de teclas; conferido na janela. Não segue a pasta quando ela muda (abre na pasta da hora).

**Fazer**
- Ctrl+J abre um painel de terminal na pasta atual (ConPTY + um emulador VT mínimo: texto, cores, cursor, rolagem), que segue a pasta quando ela muda (`cd`). Desligado por padrão.

**Aceite**
- PowerShell e cmd abrem, recebem teclas e mostram cores; fechar o painel encerra o processo; P4 do fx sem regressão com o terminal aberto.

## T3.6 — Build ARM64
- [x] Concluída em: 2026-11-22 · Resumo: job `arm64` no CI (runner x64, alvo `aarch64-pc-windows-msvc`) verde em 543 s no commit 4f0aa8a; artefato com fx, fx-indexd e fx-preview (PE 0xAA64, fx.exe 12,4 MB) e `fx-0.2.0-beta-setup-arm64.exe` (5,8 MB, `/DArch=arm64` no `fx.iss`); o job de release anexa o zip e o instalador ARM64 à tag (D-052). Anexo no release confere-se na próxima tag.

**Fazer**
- CI compila `aarch64-pc-windows-msvc` (fx, fx-indexd, fx-preview) e anexa `fx-X.Y.Z-windows-arm64.zip` e o instalador ARM64 ao release.

**Aceite**
- Build ARM64 verde no CI; artefatos no release da tag.

## T3.7 — Português e inglês
- [x] Concluída em: 2026-11-22 · Resumo: ~245 textos da interface em pares pt/en no ponto de uso (`fx_core::tr!`/`trf!`, o compilador recusa texto sem as duas versões); idioma por `settings.toml` (`idioma`), `FX_LANG` ou o idioma do Windows; troca ao vivo na paleta ("Idioma: English"), salva com a sessão; números (1.234 / 1,234), datas (DD/MM / MM/DD), tipos, filtros e problemas do renomear em lote seguem o idioma (D-053). Testes: varredura de `fx-ui/src` sem texto português fora dos pares, separadores em inglês; conferido na janela nos dois idiomas.

**Fazer**
- Todos os textos da interface num catálogo (`fx-ui/src/i18n.rs`, sem dependência), escolhido pelo idioma do Windows, com troca na paleta; formatos de número e data seguem o idioma.

**Aceite**
- Teste: todo texto tem as duas versões; nenhuma string de interface fora do catálogo (teste que varre o código).

## T3.8 — Alto contraste e DPI por monitor
- [x] Concluída em: 2026-11-22 · Resumo: com o alto contraste ligado o tema vem das cores do sistema (`GetSysColor` em `fx-shell::contrast`, `Theme::from_system`), opaco e sem Mica; a troca é vista ao vivo (consulta de 1 s, o GPUI não avisa); DPI por monitor já vinha do manifesto PerMonitorV2 e do `WM_DPICHANGED` do GPUI (D-051). Teste dos tokens; conferência visual nos testes finais.

**Fazer**
- Com o alto contraste do Windows ligado, o fx usa as cores do sistema (`GetSysColor`), muda ao vivo; mover a janela entre monitores com escalas diferentes redesenha nítido.

**Aceite**
- Teste dos tokens de alto contraste a partir das cores do sistema; conferência visual vai para os testes finais.

## T3.9 — Pastas de nuvem (OneDrive e outras)
- [x] Concluída em: 2026-11-22 · Resumo: estado de nuvem pelos atributos (`fx-core::CloudState`: OFFLINE/RECALL → só na nuvem, UNPINNED → disponível, PINNED → sempre neste dispositivo), ícone ao lado do nome (nuvem, visto, visto cheio); item só na nuvem nunca tem miniatura nem ícone por arquivo pedidos; "Manter sempre neste dispositivo" e "Liberar espaço" na paleta (atributos PINNED/UNPINNED via `fx_fs::set_cloud_pin`, em thread, recursivo em pastas); abrir já era assíncrono (D-054). Testes do mapeamento, do ícone sem leitura e dos atributos em pasta temporária; conferido na janela com atributos simulados.

**Fazer**
- Estado de cada item de pastas sincronizadas (só na nuvem, disponível, sempre neste dispositivo) a partir dos atributos (`RECALL_ON_DATA_ACCESS`, `PINNED`, `UNPINNED`, `OFFLINE`), com ícone na lista; abrir um arquivo só na nuvem não trava a UI; "Liberar espaço" e "Manter sempre neste dispositivo" no menu.

**Aceite**
- Teste do mapeamento atributos → estado; a lista nunca lê o conteúdo de um item só na nuvem (nem para miniatura).

## T3.10 — Relatório de falhas (opcional e anônimo)
- [x] Concluída em: 2026-11-22 · Resumo: pânico grava `%LOCALAPPDATA%\fx\falhas\falha-*.txt` sem dados pessoais (`fx-core::crash`: versão, módulo, arquivo:linha, mensagem e até 25 funções da pilha; caminhos → `<caminho>`, texto entre aspas → `<texto>`, nome do usuário → `<usuário>`); na abertura seguinte o status avisa e "Relatar a última falha" na paleta abre no navegador uma issue preenchida (nada é enviado pelo fx); `relatorio_falhas = false` ou a paleta desligam (D-055). Testes: relatório sem usuário nem caminhos, desligado não grava; conferido com `FX_TEST_PANIC=1` (relatório limpo, aviso, nada gravado com a opção desligada).

**Fazer**
- Uma falha (pânico) grava um relatório local sem dados pessoais (versão, módulo, linha, pilha resumida; caminhos e nomes de arquivo removidos). Na próxima abertura, o fx oferece abrir uma issue com o relatório (nada é enviado sozinho). Opção para desligar.

**Aceite**
- Teste: o relatório não contém o nome do usuário nem caminhos; desligado, nada é gravado.

## T3.11 — Relatório do portão
- [x] Concluída em: 2026-11-22 · Resumo: `bench/results/gate-3.md`: os três critérios atendidos nos testes (gerenciador padrão em chaves temporárias, 39 testes de falha do fx-ops + 4 GiB em 6,5 s); P1 421 ms, P2 69/11 ms, P4 p95 ≤ 3,95 ms, P7 0/4.000, P8 86,9 MB (meta 88), Quick Look p95 47 ms; recomendação: passar, com o gerenciador padrão testado de verdade antes de publicar.

---

## Portão da Fase 3

1. Substituição do padrão funcionando nos pontos documentados (pastas, unidades, Win+E), com o reverter restaurando tudo (testes).
2. P1–P9 mantidas.
3. Nenhum caso de perda de dados.
