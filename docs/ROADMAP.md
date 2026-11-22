# Roteiro

O fx é construído em fases. Cada fase tem um arquivo com tarefas pequenas, em ordem e com aceite verificável (`docs/phases/PHASE-N.md`), e termina num **portão**: um relatório em `bench/results/gate-N.md` com as metas medidas, os riscos e uma recomendação. A decisão de passar o portão é sempre do usuário.

| Fase | Tema | Estado | Portão |
|---|---|---|---|
| [0](phases/PHASE-0.md) | Prova de velocidade: Rust + GPUI + leitura da MFT batem as metas | ✅ 2026-11-21 | [gate-0](../bench/results/gate-0.md) |
| [1](phases/PHASE-1.md) | Navegação: abas, migalhas, lista e grade, operações com desfazer, menu do shell | ✅ 2026-11-21 | [gate-1](../bench/results/gate-1.md) |
| [2](phases/PHASE-2.md) | Busca no disco inteiro, Quick Look, paleta, renomear em lote, beta | ✅ 2026-11-22 | [gate-2](../bench/results/gate-2.md) |
| [3](phases/PHASE-3.md) | Virar o padrão, painel duplo, colunas, etiquetas, terminal, ARM64, idiomas, alto contraste, nuvem, falhas | ✅ 2026-11-22 | [gate-3](../bench/results/gate-3.md) |
| [4](phases/PHASE-4.md) | Atualizador, polimento, instalador próprio, telas "Sobre" e "Como usar" | ✅ 2026-11-22 | [gate-4](../bench/results/gate-4.md) |
| [5](phases/PHASE-5.md) | Pendências de todas as fases e lançamento da 1.0 | ⏳ não iniciada | — |

## O que cada fase entregou

- **Fase 0:** leitura de pasta com `FindFirstFileEx` em lote, lista virtualizada de 100 mil itens, índice da MFT de 763 mil arquivos em 7,6 s e busca por tecla em 4 ms. Decisões de base: GPUI com patch próprio (D-007), compilação cruzada pelo WSL (D-010), só Windows por enquanto (D-012).
- **Fase 1:** a navegação completa de um gerenciador de arquivos, com fila de operações, conflitos e desfazer, arrastar e soltar, sessão restaurada e leitor de tela. O portão de 2 semanas de uso diário foi dispensado pelo usuário (D-035).
- **Fase 2:** serviço `fx-indexd` com busca no disco inteiro por pipe, filtros digitados, Quick Look com visualizadores do Windows num processo isolado, renomear em lote, tamanho das pastas, testes de falha do motor de operações e o primeiro instalador.
- **Fase 3:** gerenciador padrão com reverter, painel duplo, vista em colunas, etiquetas, terminal embutido, build ARM64, português e inglês, alto contraste, pastas de nuvem e relatório de falhas.
- **Fase 4:** atualizador que baixa e aplica ao fechar (como o Zed), instalador próprio `fx-setup` com a interface do fx, transições, estados vazios, mensagens de erro nos dois idiomas, telas "Sobre" e "Como usar". O fx foi instalado no PC do usuário.

## Fase 5 — o que falta para a 1.0

Detalhada em [`phases/PHASE-5.md`](phases/PHASE-5.md): testes manuais com o usuário ([`TESTES-FINAIS.md`](TESTES-FINAIS.md)), assinatura digital e publicação (GitHub, Microsoft Store, winget), licenças de terceiros, site e guia de uso, terminal completo, pendências de funcionalidades e robustez, e margem para a meta de memória (P8).

## Depois da 1.0

Fora do escopo até aqui, registrados para decidir depois: porte para macOS e Linux (a interface e o instalador já usam GPUI; falta a camada do sistema, D-012), inspetor lateral de metadados, e plugins.
