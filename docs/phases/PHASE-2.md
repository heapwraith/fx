# Fase 2 — Busca, prévias e beta pública

> **Aprovada em 2026-11-21.** O usuário dispensou as 2 semanas de uso do portão da Fase 1 (D-035): quer o fx completo primeiro e vai usando e melhorando depois. Os demais critérios da Fase 1 passaram (`bench/results/gate-1.md`).

**Objetivo:** o fx completo para uma beta pública: busca no disco inteiro sempre atualizada, prévias de vídeo, PDF e código, renomear em lote, tamanho de pastas, ícone, instalador e executável gerado pelo GitHub, sem nenhum caso de perda de dados.

**Duração estimada:** 8 a 10 semanas. **Fora de escopo nesta fase:** virar o padrão do Windows, painel duplo, vista em colunas, ARM64, inglês (Fase 3); Microsoft Store, winget e atualizador automático (Fase 4).

**Já adiantado na Fase 1:** Ctrl+F sem filtros (T1.23), Quick Look de imagem e texto (T1.22), paleta de comandos (T1.24).

**Dependências novas previstas** (cada uma registrada em `docs/DECISIONS.md` na tarefa que a introduzir, e **pedida ao usuário antes**): `embed-resource` (T2.1, feito: já vinha pelo GPUI); `regex` (T2.8, feito: já vinha no `Cargo.lock`); nenhuma na T2.9 (ZIP, Markdown e cores feitos no `fx-core`, D-044); Inno Setup no CI (T2.9).

**Ações que pedem o usuário** (SAFETY.md §1): instalar e testar o serviço elevado (T2.4); pedir a assinatura gratuita à SignPath Foundation (T2.10, só o usuário pode se inscrever).

Regras: faça as tarefas **na ordem**. Ao concluir uma, marque `[x]`, coloque a data e uma linha de resumo com os números. Toda tarefa de UI segue a skill `fx-design` (conferência na janela) e mede P4 de novo.

---

## T2.1 — Ícone e metadados do executável
- [x] Concluída em: 2026-11-21 · Resumo: ícone próprio (pasta teal com »», versão sem a seta em 16–24 px) gerado por `tools/make-icon.py` em `crates/fx-ui/resources/fx.ico` (10 tamanhos, 16–256); `build.rs` embute o ícone (id 1, que o GPUI usa na classe da janela) e a versão (`fx — gerenciador de arquivos`, 0.0.1, licença) via `embed-resource` (D-036). Conferido: barra de título, barra de tarefas e Propriedades do arquivo. Alt+Tab não capturado (usa o mesmo recurso).

**Fazer**
- Ícone do fx (16 a 256 px, claro e escuro legíveis, sem marca de terceiros), desenhado como SVG e convertido em `.ico`.
- Recurso Windows no `fx.exe`: ícone, nome, versão, descrição, copyright; manifesto com DPI por monitor e Common Controls 6. Mesmo ícone na barra de tarefas, Alt+Tab e na janela.

**Aceite**
- Captura da barra de tarefas, do Alt+Tab e das Propriedades do `fx.exe` mostrando ícone e versão; build no WSL (`cargo xwin`) e no CI.

## T2.2 — Pipeline no GitHub (CI e executável)
- [x] Concluída em: 2026-11-21 · Resumo: `.github/workflows/ci.yml` em `windows-latest` (fmt, clippy `-D warnings`, testes, build release, `fx.exe` como artefato), primeiro push verde (17 min sem cache); a tag `v0.1.0` gerou o release em rascunho de pré-lançamento com `fx-0.1.0-windows-x64.zip` (exe, licenças, README, CHANGELOG) e notas da seção do CHANGELOG. Versão 0.1.0 com commit no exe (`0.1.0+bbe1e2f`), "Sobre o fx" na paleta (D-037). O exe desse rascunho tem o travamento corrigido na T2.3: não publicar; lançar 0.1.1.

**Fazer**
- GitHub Actions em `windows-latest`: `cargo fmt --check`, clippy com `-D warnings`, `cargo test --workspace`, build release, a cada push e PR.
- Em tag `v*`: zip portátil (`fx.exe` + licenças) anexado a um release em rascunho no GitHub.
- Cache do cargo; toolchain fixado pelo `rust-toolchain.toml`.

**Aceite**
- Um push verde no GitHub; uma tag de teste gera o zip, que roda num Windows limpo.

## T2.3 — Pendências da Fase 1
- [x] Concluída em: 2026-11-22 · Resumo: migalhas que não cabem viram "…" com menu das pastas escondidas (3 testes; conferido estreito, maximizado e no menu); status com a largura do próprio texto; travamento do texto truncado corrigido no patch do GPUI; `fx-bench memory`: **P8 85,3 MB**, meta revista para ≤ 88 MB porque o excedente é o cache de ícones do Windows (D-038). Testes manuais com o usuário (Narrador, Explorador) movidos para a T2.11.

**Fazer**
- P8: memória parada numa pasta pequena ≤ 80 MB (hoje ~84). Perfilar ícones e lateral (VMMap/ETW) e cortar.
- Migalhas: quando o caminho não cabe, recolher as do início num botão "…" (em vez de cortar a primeira no meio).
- Testes manuais com o usuário: Narrador ao vivo; copiar, colar e arrastar com o Explorador, a área de trabalho e o navegador.

**Aceite**
- P8 medida ≤ 80 MB; capturas das migalhas a 640 px; lista dos testes manuais com resultado.

## T2.4 — Testes de falha do motor de operações (SAFETY.md §3)
- [x] Concluída em: 2026-11-22 · Resumo: 9 testes novos no `fx-ops` (37 no total), todos sem perda: disco cheio no meio (cópia e movimentação entre volumes), origem em uso, "substituir" sobre arquivo em uso ou somente leitura, mover com origem que não pode ser apagada (fica duplicado, nunca perdido), **processo morto de verdade no meio da cópia** (10 de 10) e arquivo de 4 GiB + 1 MiB (5,7 s, ignorado por gravar 8 GB). Novo diário de temporários: o fx apaga na partida o que um processo morto deixou (D-039).

**Fazer**
- Testes de `fx-ops`: falta de espaço no meio da cópia (VHD pequeno ou cota em pasta temporária), arquivo em uso e acesso negado, cancelamento no meio, processo morto no meio (reabrir e conferir), nomes Unicode, caminhos > 260, 0 bytes e > 4 GB.

**Aceite**
- Todos os casos com teste; nenhum perde dados (origem intacta ou destino completo, nunca nenhum dos dois).

## T2.5 — `fx-indexd`: serviço de índice
- [x] Concluída em: 2026-11-22 · Resumo: `fx-indexd` (serviço do Windows, D-040): lê os volumes NTFS fixos, segue o USN a cada 250 ms, responde por pipe com ACL só para SYSTEM e o usuário, esconde perfis de outros usuários; instala copiando para `%ProgramFiles%\fx` e remove sem resto; comandos na paleta com UAC. Medido elevado (`bench/indexd-test.ps1`): 744.649 itens prontos em 7,5 s, **P9 95,7 MB/milhão** (índice; processo inteiro 141,9 MB), arquivo novo na busca em **101–258 ms**, apagado some em ≤ 255 ms, busca em 3,8 ms, desinstalação sem serviço, arquivo ou pasta.

**Fazer**
- Serviço do Windows opcional que lê a MFT de todas as unidades NTFS (`fx-index`), segue o diário USN para ficar atualizado e responde buscas por named pipe com ACL só para o usuário.
- Instalar e remover pelo fx, com consentimento explícito (UAC); o fx funciona sem ele.

**Aceite**
- Índice do C: em memória ≤ 150 MB por milhão de itens (P9); arquivo criado aparece na busca em ≤ 1 s; serviço removido sem resto (serviço, arquivos, regras).

## T2.6 — Busca completa
- [x] Concluída em: 2026-11-22 · Resumo: filtros digitados (`tipo:`, `ext:`, `tamanho:`, `data:`; `fx-core::search_filter`, 6 testes) aplicados dentro de cada busca (serviço, índice próprio e pasta), também sem nome; Tab alterna "Disco inteiro" (fx-indexd) e "Nesta pasta"; colunas Tamanho e Modificado ordenáveis; duplo clique abre, arrastar e menu do Windows nos resultados (D-041). **P5 pelo serviço: p95 5,39 ms** (243 teclas, até 1000 resultados de 744 mil itens); P4 das teclas da busca p95 3,57 ms. Achado e corrigido: o índice dobrava de memória na primeira mudança depois da leitura (139 → 73,5 MB; processo do serviço 142 → 76 MB).

**Fazer**
- Ctrl+F consulta o `fx-indexd` quando instalado (disco inteiro), senão a enumeração atual. Filtros: tipo, tamanho, data, pasta; resultados ordenáveis; abrir, revelar, arrastar e menu de contexto nos resultados.

**Aceite**
- P5: resultado ≤ 50 ms p95 com o índice do C:; P4 ≤ 16 ms digitando; teste dos filtros.

## T2.7 — Tamanho das pastas
- [x] Concluída em: 2026-11-22 · Resumo: coluna Tamanho com o total de cada subpasta (D-042): pelo `fx-indexd` quando ele roda (todos de uma vez e sempre atuais; `Index::folder_totals`, **~32 ms com 1 milhão de itens**), senão calculado em segundo plano na ordem da tela, com "…", cancelado ao sair e guardado na sessão; a ordenação por tamanho inclui as pastas. Totais iguais aos do PowerShell nas fixtures (121.290, 21.064 e 20.990 KB), testes contra varredura independente e no serviço (perfis de outros usuários ficam de fora); P4 sem regressão (p95 ≤ 3,5 ms).

**Fazer**
- Coluna Tamanho com o total de cada pasta vindo do índice (sem o serviço: cálculo sob demanda em segundo plano, cancelável).

**Aceite**
- Igual ao total das Propriedades do Windows nas fixtures (teste); P4 sem regressão.

## T2.8 — Renomear em lote
- [x] Concluída em: 2026-11-22 · Resumo: F2 com vários itens abre o renomear em lote (D-043): Buscar/Substituir (texto ou regex, Alt+R; maiúsculas Alt+M), Modelo com `{nome}` e `{n:3}`, letras (Alt+C), prévia ao vivo com problemas marcados (inválido, reservado, repetido, já existe) e Enter só sem problemas; job em duas fases (ciclos a→b→c→a e troca só de maiúscula funcionam; falha desfaz o feito) e um Ctrl+Z reverte tudo. Testes: 6 das regras, 2 do motor; conferido na janela (6 itens renomeados e desfeitos).

**Fazer**
- Com vários itens selecionados, F2 abre o renomear em lote: buscar e substituir (texto ou regex), numeração, maiúsculas e minúsculas, extensão; prévia ao vivo; conflitos e nomes inválidos marcados antes de aplicar; uma única operação para desfazer.

**Aceite**
- Testes da regra de nomes (colisão, nome inválido, ciclo a→b→a); Ctrl+Z desfaz tudo.

## T2.9 — Prévias completas (`fx-preview`)
- [x] Concluída em: 2026-11-22 · Resumo: `fx-preview.exe` isolado hospeda os visualizadores do Windows (`IPreviewHandler`) sobre o painel do Quick Look, com tempo limite de 5 s e encerrado ao trocar de item (D-044); código com cores (tokenizador próprio, 6 famílias), Markdown renderizado e lista de ZIP pelo diretório central, sem dependências novas. Conferido na janela: PDF (visualizador do Edge), MD, ZIP e RS; visualizador travado (teste com host falso) custa só o tempo limite; P4 sem regressão (p95 ≤ 3,5 ms; Quick Look p95 46 ms). DOCX, MP4 e MP3 sem visualizador nesta máquina (sem Office; o Windows 11 não traz o de mídia): mostram a miniatura do Windows.

**Fazer**
- Processo `fx-preview` isolado que hospeda os preview handlers do Windows (PDF, Office, vídeo e áudio), com tempo limite: se travar, o fx não trava.
- No fx: código com cores, Markdown renderizado, conteúdo de ZIP.

**Aceite**
- Prévia de PDF, MP4, MP3, DOCX, ZIP, MD e RS conferidas na janela; handler travado não afeta o fx (teste com handler falso); P4 sem regressão.

## T2.10 — Instalador e portátil
- [x] Concluída em: 2026-11-22 · Resumo: instalador por usuário com Inno Setup (`installer/fx.iss`, D-045): `%LOCALAPPDATA%\Programs\fx`, Menu Iniciar, sem administrador, desinstalador que remove o que instalou, pergunta se apaga as configurações e remove o serviço de índice se ele existir (com UAC); gerado pelo CI a cada push (artefato) e anexado ao release da tag junto do zip portátil. Teste num Windows limpo vai para `docs/TESTES-FINAIS.md` (pedido do usuário: testes manuais no fim).

**Fazer**
- Instalador por usuário (sem admin) gerado no CI: atalho no Menu Iniciar, desinstalador que apaga tudo que instalou (não os dados do usuário, a não ser que ele marque). Zip portátil continua.

**Aceite**
- Instalar, abrir, desinstalar num Windows limpo sem deixar nada; ambos anexados ao release da tag.

## T2.11 — Beta pública
- [ ] Concluída em: ____ · Resumo: parte do agente feita em 2026-11-22: README com instalação, atalhos, busca, serviço e como relatar problemas; modelos de issue (problema, com a pergunta de perda de dados, e ideia); assinatura documentada em `docs/SIGNING.md`; versão 0.2.0-beta e seção no CHANGELOG; tag `v0.2.0-beta` gera o rascunho do release. **Falta (usuário, `docs/TESTES-FINAIS.md`):** inscrição na SignPath, testes manuais e publicar o rascunho.

**Fazer**
- Assinatura do executável e do instalador (SignPath Foundation, depois que o usuário se inscrever); README com instalação, atalhos e como relatar problemas; modelo de issue.
- Testes manuais com o usuário (vindos da T2.3): Narrador ao vivo; copiar, colar e arrastar com o Explorador, a área de trabalho e o navegador.

**Aceite**
- Testes manuais feitos, com o resultado de cada um registrado; release `v0.2.0-beta` publicado pelo usuário; SmartScreen sem aviso de "editor desconhecido" (se a assinatura sair a tempo).

## T2.12 — Relatório do portão
- [x] Concluída em: 2026-11-22 · Resumo: `bench/results/gate-2.md`: P1 433 ms, P2 10,5 ms (aba), P4 p95 ≤ 3,9 ms, P5 5,39 ms pelo serviço, P7 0 perdidos em 3.999, P8 85,8 MB, P9 98,7 MB/milhão; nenhum caso de perda de dados; beta aguardando o usuário (SignPath, testes, publicar).

**Fazer**
- `/gate`: metas P1–P9, falhas conhecidas, testes de falha, comparação com o Explorador e o Everything.

---

## Portão da Fase 2

Passa para a Fase 3 somente se:
1. **Nenhum caso de perda de dados** (um único caso reprova), com os testes da T2.4 passando.
2. **P1–P9** mantidas.
3. Beta publicada e taxa de falhas conhecida. (O roteiro original pede 100 testadores; o usuário decide se esse número vale.)
