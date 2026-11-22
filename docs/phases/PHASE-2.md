# Fase 2 — Busca, prévias e beta pública

> **Aprovada em 2026-11-21.** O usuário dispensou as 2 semanas de uso do portão da Fase 1 (D-035): quer o fx completo primeiro e vai usando e melhorando depois. Os demais critérios da Fase 1 passaram (`bench/results/gate-1.md`).

**Objetivo:** o fx completo para uma beta pública: busca no disco inteiro sempre atualizada, prévias de vídeo, PDF e código, renomear em lote, tamanho de pastas, ícone, instalador e executável gerado pelo GitHub, sem nenhum caso de perda de dados.

**Duração estimada:** 8 a 10 semanas. **Fora de escopo nesta fase:** virar o padrão do Windows, painel duplo, vista em colunas, ARM64, inglês (Fase 3); Microsoft Store, winget e atualizador automático (Fase 4).

**Já adiantado na Fase 1:** Ctrl+F sem filtros (T1.23), Quick Look de imagem e texto (T1.22), paleta de comandos (T1.24).

**Dependências novas previstas** (cada uma registrada em `docs/DECISIONS.md` na tarefa que a introduzir, e **pedida ao usuário antes**): `embed-resource` ou chamada direta ao `llvm-rc` (T2.1); `zip` (T2.8); `pulldown-cmark` (T2.8); `tree-sitter` e gramáticas (T2.8, avaliar o peso no executável); Inno Setup no CI (T2.9).

**Ações que pedem o usuário** (SAFETY.md §1): instalar e testar o serviço elevado (T2.4); pedir a assinatura gratuita à SignPath Foundation (T2.10, só o usuário pode se inscrever).

Regras: faça as tarefas **na ordem**. Ao concluir uma, marque `[x]`, coloque a data e uma linha de resumo com os números. Toda tarefa de UI segue a skill `fx-design` (conferência na janela) e mede P4 de novo.

---

## T2.1 — Ícone e metadados do executável
- [ ] Concluída em: ____ · Resumo:

**Fazer**
- Ícone do fx (16 a 256 px, claro e escuro legíveis, sem marca de terceiros), desenhado como SVG e convertido em `.ico`.
- Recurso Windows no `fx.exe`: ícone, nome, versão, descrição, copyright; manifesto com DPI por monitor e Common Controls 6. Mesmo ícone na barra de tarefas, Alt+Tab e na janela.

**Aceite**
- Captura da barra de tarefas, do Alt+Tab e das Propriedades do `fx.exe` mostrando ícone e versão; build no WSL (`cargo xwin`) e no CI.

## T2.2 — Pipeline no GitHub (CI e executável)
- [ ] Concluída em: ____ · Resumo:

**Fazer**
- GitHub Actions em `windows-latest`: `cargo fmt --check`, clippy com `-D warnings`, `cargo test --workspace`, build release, a cada push e PR.
- Em tag `v*`: zip portátil (`fx.exe` + licenças) anexado a um release em rascunho no GitHub.
- Cache do cargo; toolchain fixado pelo `rust-toolchain.toml`.

**Aceite**
- Um push verde no GitHub; uma tag de teste gera o zip, que roda num Windows limpo.

## T2.3 — Pendências da Fase 1
- [ ] Concluída em: ____ · Resumo:

**Fazer**
- P8: memória parada numa pasta pequena ≤ 80 MB (hoje ~84). Perfilar ícones e lateral (VMMap/ETW) e cortar.
- Migalhas: quando o caminho não cabe, recolher as do início num botão "…" (em vez de cortar a primeira no meio).
- Testes manuais com o usuário: Narrador ao vivo; copiar, colar e arrastar com o Explorador, a área de trabalho e o navegador.

**Aceite**
- P8 medida ≤ 80 MB; capturas das migalhas a 640 px; lista dos testes manuais com resultado.

## T2.4 — Testes de falha do motor de operações (SAFETY.md §3)
- [ ] Concluída em: ____ · Resumo:

**Fazer**
- Testes de `fx-ops`: falta de espaço no meio da cópia (VHD pequeno ou cota em pasta temporária), arquivo em uso e acesso negado, cancelamento no meio, processo morto no meio (reabrir e conferir), nomes Unicode, caminhos > 260, 0 bytes e > 4 GB.

**Aceite**
- Todos os casos com teste; nenhum perde dados (origem intacta ou destino completo, nunca nenhum dos dois).

## T2.5 — `fx-indexd`: serviço de índice
- [ ] Concluída em: ____ · Resumo:

**Fazer**
- Serviço do Windows opcional que lê a MFT de todas as unidades NTFS (`fx-index`), segue o diário USN para ficar atualizado e responde buscas por named pipe com ACL só para o usuário.
- Instalar e remover pelo fx, com consentimento explícito (UAC); o fx funciona sem ele.

**Aceite**
- Índice do C: em memória ≤ 150 MB por milhão de itens (P9); arquivo criado aparece na busca em ≤ 1 s; serviço removido sem resto (serviço, arquivos, regras).

## T2.6 — Busca completa
- [ ] Concluída em: ____ · Resumo:

**Fazer**
- Ctrl+F consulta o `fx-indexd` quando instalado (disco inteiro), senão a enumeração atual. Filtros: tipo, tamanho, data, pasta; resultados ordenáveis; abrir, revelar, arrastar e menu de contexto nos resultados.

**Aceite**
- P5: resultado ≤ 50 ms p95 com o índice do C:; P4 ≤ 16 ms digitando; teste dos filtros.

## T2.7 — Tamanho das pastas
- [ ] Concluída em: ____ · Resumo:

**Fazer**
- Coluna Tamanho com o total de cada pasta vindo do índice (sem o serviço: cálculo sob demanda em segundo plano, cancelável).

**Aceite**
- Igual ao total das Propriedades do Windows nas fixtures (teste); P4 sem regressão.

## T2.8 — Renomear em lote
- [ ] Concluída em: ____ · Resumo:

**Fazer**
- Com vários itens selecionados, F2 abre o renomear em lote: buscar e substituir (texto ou regex), numeração, maiúsculas e minúsculas, extensão; prévia ao vivo; conflitos e nomes inválidos marcados antes de aplicar; uma única operação para desfazer.

**Aceite**
- Testes da regra de nomes (colisão, nome inválido, ciclo a→b→a); Ctrl+Z desfaz tudo.

## T2.9 — Prévias completas (`fx-preview`)
- [ ] Concluída em: ____ · Resumo:

**Fazer**
- Processo `fx-preview` isolado que hospeda os preview handlers do Windows (PDF, Office, vídeo e áudio), com tempo limite: se travar, o fx não trava.
- No fx: código com cores, Markdown renderizado, conteúdo de ZIP.

**Aceite**
- Prévia de PDF, MP4, MP3, DOCX, ZIP, MD e RS conferidas na janela; handler travado não afeta o fx (teste com handler falso); P4 sem regressão.

## T2.10 — Instalador e portátil
- [ ] Concluída em: ____ · Resumo:

**Fazer**
- Instalador por usuário (sem admin) gerado no CI: atalho no Menu Iniciar, desinstalador que apaga tudo que instalou (não os dados do usuário, a não ser que ele marque). Zip portátil continua.

**Aceite**
- Instalar, abrir, desinstalar num Windows limpo sem deixar nada; ambos anexados ao release da tag.

## T2.11 — Beta pública
- [ ] Concluída em: ____ · Resumo:

**Fazer**
- Assinatura do executável e do instalador (SignPath Foundation, depois que o usuário se inscrever); README com instalação, atalhos e como relatar problemas; modelo de issue.

**Aceite**
- Release `v0.2.0-beta` publicado pelo usuário; SmartScreen sem aviso de "editor desconhecido" (se a assinatura sair a tempo).

## T2.12 — Relatório do portão
- [ ] Concluída em: ____ · Resumo:

**Fazer**
- `/gate`: metas P1–P9, falhas conhecidas, testes de falha, comparação com o Explorador e o Everything.

---

## Portão da Fase 2

Passa para a Fase 3 somente se:
1. **Nenhum caso de perda de dados** (um único caso reprova), com os testes da T2.4 passando.
2. **P1–P9** mantidas.
3. Beta publicada e taxa de falhas conhecida. (O roteiro original pede 100 testadores; o usuário decide se esse número vale.)
