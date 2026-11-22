# Fase 3 — Virar o padrão

> **Iniciada em 2026-11-22**, a pedido do usuário: terminar as funcionalidades das fases restantes e fazer os testes manuais e as correções no fim (`docs/TESTES-FINAIS.md`). O portão da Fase 2 está em `bench/results/gate-2.md`.

**Objetivo:** o fx pode substituir o Explorador (com volta garantida), ganha os modos de navegação de usuário avançado (painel duplo, colunas, etiquetas, terminal), roda em ARM64, fala português e inglês, respeita alto contraste e DPI por monitor, mostra o estado das pastas de nuvem e, se o usuário quiser, guarda relatórios de falha.

**Regras desta fase:**
- **Registro e Win+E** (T3.1): o código só altera `HKCU` e sempre tem um "reverter" que restaura o que havia antes. Os testes usam uma chave temporária própria. **Aplicar no PC do usuário só quando ele pedir** (SAFETY.md §1).
- Sem dependências novas sem registro em `docs/DECISIONS.md`.
- Toda tarefa de UI segue a skill `fx-design` e mede P4; testes manuais vão para `docs/TESTES-FINAIS.md`.

---

## T3.1 — Gerenciador de arquivos padrão (com reverter)
- [ ] Concluída em: ____ · Resumo:

**Fazer**
- Em `fx-shell`: estado, ativar e reverter. Ativar faz pastas, unidades e Win+E abrirem no fx, por chaves em `HKCU\Software\Classes` (verbo `open` de `Folder`/`Directory`/`Drive` e o comando de Win+E). Antes de gravar, faz uma cópia do que havia. Reverter apaga só o que o fx criou e restaura a cópia.
- No fx: comandos na paleta ("Tornar o fx o gerenciador padrão" / "Voltar ao Explorador") e o estado em "Sobre o fx".

**Aceite**
- Testes com uma raiz temporária (`HKCU\Software\fx-test-<pid>`): ativar grava o esperado, reverter deixa a raiz como estava (inclusive valores pré-existentes); nenhum teste toca as chaves reais.

## T3.2 — Painel duplo (F3)
- [ ] Concluída em: ____ · Resumo:

**Fazer**
- F3 divide a aba em dois painéis lado a lado, cada um com sua pasta, histórico e seleção; Tab alterna o foco; F5/F6 copiam/movem a seleção para a pasta do outro painel (mesmo motor e conflitos). O estado do painel duplo é salvo por aba.

**Aceite**
- Copiar e mover entre painéis pelo motor (teste); P4 sem regressão.

## T3.3 — Vista em colunas (Miller)
- [ ] Concluída em: ____ · Resumo:

**Fazer**
- Ctrl+Shift+3: colunas lado a lado, uma por nível do caminho; selecionar uma pasta abre a coluna seguinte; ← → navegam entre colunas. Cada coluna é uma lista virtualizada lida em segundo plano.

**Aceite**
- Abrir uma pasta de 100 mil numa coluna sem travar (P4); teste do modelo de colunas (caminho → colunas, seleção, voltar).

## T3.4 — Etiquetas
- [ ] Concluída em: ____ · Resumo:

**Fazer**
- Até 7 etiquetas coloridas por item (nomes editáveis), guardadas no banco do fx (por caminho, seguindo renomeações e movimentos feitos pelo fx); coluna/ponto de cor na lista; filtrar por etiqueta na busca (`etiqueta:`) e na paleta.

**Aceite**
- Testes do banco (atribuir, remover, renomear arquivo, mover); filtro na busca.

## T3.5 — Terminal embutido (opcional)
- [ ] Concluída em: ____ · Resumo:

**Fazer**
- Ctrl+J abre um painel de terminal na pasta atual (ConPTY + um emulador VT mínimo: texto, cores, cursor, rolagem), que segue a pasta quando ela muda (`cd`). Desligado por padrão.

**Aceite**
- PowerShell e cmd abrem, recebem teclas e mostram cores; fechar o painel encerra o processo; P4 do fx sem regressão com o terminal aberto.

## T3.6 — Build ARM64
- [ ] Concluída em: ____ · Resumo:

**Fazer**
- CI compila `aarch64-pc-windows-msvc` (fx, fx-indexd, fx-preview) e anexa `fx-X.Y.Z-windows-arm64.zip` e o instalador ARM64 ao release.

**Aceite**
- Build ARM64 verde no CI; artefatos no release da tag.

## T3.7 — Português e inglês
- [ ] Concluída em: ____ · Resumo:

**Fazer**
- Todos os textos da interface num catálogo (`fx-ui/src/i18n.rs`, sem dependência), escolhido pelo idioma do Windows, com troca na paleta; formatos de número e data seguem o idioma.

**Aceite**
- Teste: todo texto tem as duas versões; nenhuma string de interface fora do catálogo (teste que varre o código).

## T3.8 — Alto contraste e DPI por monitor
- [ ] Concluída em: ____ · Resumo:

**Fazer**
- Com o alto contraste do Windows ligado, o fx usa as cores do sistema (`GetSysColor`), muda ao vivo; mover a janela entre monitores com escalas diferentes redesenha nítido.

**Aceite**
- Teste dos tokens de alto contraste a partir das cores do sistema; conferência visual vai para os testes finais.

## T3.9 — Pastas de nuvem (OneDrive e outras)
- [ ] Concluída em: ____ · Resumo:

**Fazer**
- Estado de cada item de pastas sincronizadas (só na nuvem, disponível, sempre neste dispositivo) a partir dos atributos (`RECALL_ON_DATA_ACCESS`, `PINNED`, `UNPINNED`, `OFFLINE`), com ícone na lista; abrir um arquivo só na nuvem não trava a UI; "Liberar espaço" e "Manter sempre neste dispositivo" no menu.

**Aceite**
- Teste do mapeamento atributos → estado; a lista nunca lê o conteúdo de um item só na nuvem (nem para miniatura).

## T3.10 — Relatório de falhas (opcional e anônimo)
- [ ] Concluída em: ____ · Resumo:

**Fazer**
- Uma falha (pânico) grava um relatório local sem dados pessoais (versão, módulo, linha, pilha resumida; caminhos e nomes de arquivo removidos). Na próxima abertura, o fx oferece abrir uma issue com o relatório (nada é enviado sozinho). Opção para desligar.

**Aceite**
- Teste: o relatório não contém o nome do usuário nem caminhos; desligado, nada é gravado.

## T3.11 — Relatório do portão
- [ ] Concluída em: ____ · Resumo:

---

## Portão da Fase 3

1. Substituição do padrão funcionando nos pontos documentados (pastas, unidades, Win+E), com o reverter restaurando tudo (testes).
2. P1–P9 mantidas.
3. Nenhum caso de perda de dados.
