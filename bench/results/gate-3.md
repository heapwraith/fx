# Portão da Fase 3 — relatório

**Data:** 2026-11-22 · **Versão:** 0.2.0-beta (commit f5f3864) · **Build:** release, compilado no WSL com `cargo xwin`, executado no Windows; CI verde no GitHub (`check` e `arm64`).

## Máquina

A mesma das fases anteriores: AMD Ryzen 7 5700X, 32 GB, RTX 5070, monitor 1920×1080 a 239 Hz (escala 100%), Kingston NV2 NVMe, Windows 11 25H2 (exibição em inglês). Fixtures em `%TEMP%\fx-fixtures`, cache quente. O serviço de índice (`fx-indexd`) não estava instalado durante as medições.

## Tarefas

| Tarefa | Estado |
|---|---|
| T3.1 Gerenciador padrão com reverter | ✅ (testado em chaves temporárias; aplicar no PC do usuário fica para os testes finais) |
| T3.2 Painel duplo | ✅ |
| T3.3 Vista em colunas | ✅ |
| T3.4 Etiquetas | ✅ |
| T3.5 Terminal embutido | ✅ parcial: não segue a pasta da aba depois de aberto (`cd`) e só PowerShell; P4 com o terminal aberto não foi medido |
| T3.6 Build ARM64 | ✅ CI verde; anexo no release confere-se na próxima tag |
| T3.7 Português e inglês | ✅ (mensagens de erro de `fx-ops`/`fx-shell` seguem em português) |
| T3.8 Alto contraste e DPI | ✅ (conferência visual nos testes finais) |
| T3.9 Pastas de nuvem | ✅ com atributos simulados (OneDrive real nos testes finais) |
| T3.10 Relatório de falhas | ✅ |
| T3.11 Este relatório | ✅ |

## Critérios do portão

| Critério | Resultado | Passou? |
|---|---|---|
| **1. Substituição do padrão** nos pontos documentados (pastas, unidades, Win+E), com o reverter restaurando tudo | 3 testes de `default_manager` passando em raiz temporária `HKCU\Software\fx-test-*`: ativar e reverter numa raiz vazia não deixa nada; reverter restaura os valores que existiam antes (backup); as chaves reais só são nomeadas pelo app. Comandos na paleta. **Não aplicado no registro real** do usuário (SAFETY.md): fica em `docs/TESTES-FINAIS.md` | ✅ nos testes |
| **2. P1–P9 mantidas** | todas dentro da meta (tabela abaixo); P5 pelo serviço e P6 no disco real não refeitos (serviço desinstalado, pede Administrador) | ✅ |
| **3. Nenhum caso de perda de dados** | `fx-ops`: 39 testes passando (disco cheio, arquivo em uso, somente leitura, mover com origem travada, processo morto no meio, diário de temporários, desfazer) + cópia de 4 GiB + 1 MiB em 6,5 s, conteúdo conferido. As funcionalidades novas que movem dados (F5/F6 do painel duplo, liberar espaço na nuvem) usam a mesma fila; liberar espaço só muda atributos | ✅ |

## Metas (medidas hoje)

| Meta | Resultado | Meta | Fase 2 | |
|---|---|---|---|---|
| P1 primeira abertura | mediana 420,9 ms, p95 428,9 | ≤ 500 ms | 433 / 439 | ✅ |
| P2 pasta de 100 mil, fx carregado | do pedido: 68,9 / 72,5 ms; aba nova: 11,0 / 13,1 ms | ≤ 100 ms | 75 / 88; 10,5 / 12,0 | ✅ |
| P3 ordenar 100 mil | por nome 39,4 ms (p95 49,7); filtro por tecla 0,84 ms | ≤ 50 ms / ≤ 5 ms | ~37 | ✅ |
| P4 tecla → quadro | paleta p95 3,24–3,45 ms; setas durante reordenação 3,05–3,23; busca com resultados chegando 3,60; abas 3,95 | ≤ 16 ms | 3,0–3,9 | ✅ |
| P5 busca | índice sintético de 2 milhões: p95 7,84 ms, máx. 10,2 (pelo serviço não refeito; Fase 2: p95 5,39 ms em 744 mil reais) | ≤ 50 ms | — | ✅ |
| P6 construir o índice | sintético 2 milhões em 1,56 s (só memória); disco real não refeito (Fase 2: 744 mil em 7,5 s) | ≤ 10 s | 7,5 s | ✅ (Fase 2) |
| P7 rolagem | **0 perdidos em 4.000** e **0 em 4.002** quadros a 239 Hz; uma primeira execução anômala deu 8 em 468 (1,7%, sessões curtas, roda chegou a 82%), ver riscos | ≤ 0,5% | 0 / 3.999 | ✅ |
| P8 memória parado | mediana 86,9 MB [86,2–88,8] | ≤ 88 MB | 85,8 | ✅ (+1,1 MB) |
| P9 índice por milhão | 71,4 MB/milhão (sintético) | ≤ 150 MB | 98,7 (real) | ✅ |
| Quick Look | p95 47,4 ms (JPEG 9,5 MB, PNG 10,4 MB, texto) | ≤ 100 ms | 46–66 | ✅ |
| Enumeração 100 mil | FindFirstFileEx 3,2 ms mediana (cache quente) | — | — | ✅ |

Nenhuma regressão acima de 10% contra a Fase 2. P8 subiu 1,1 MB (etiquetas, terminal, idioma) e ficou a 1,1 MB da meta: é a meta mais apertada.

## Riscos

1. **P8 perto do limite:** 86,9 MB com meta de 88 (uma amostra em 88,8). A Fase 4 (atualizador, polimento) não pode crescer sem compensar.
2. **Medições instáveis na primeira execução:** a primeira rodada de P7 (468 quadros em vez de ~4.000) e a primeira de busca no P4 ("nenhuma tecla registrada") falharam e passaram ao repetir. Provável: a janela do fx não recebeu foco/entrada no início, logo depois de outros benchmarks. Os scripts deveriam conferir o foco antes de começar.
3. **Testes manuais adiados** (pedido do usuário), agora 11 itens em `docs/TESTES-FINAIS.md`: Narrador, Explorador, instalador limpo, gerenciador padrão aplicado de verdade, alto contraste, OneDrive real, ARM64, terminal com cmd.
4. **Gerenciador padrão:** é a mudança mais invasiva do projeto (registro de `Folder`, `Drive`, Win+E). Testado só em chaves temporárias; o reverter guarda backup, mas o primeiro uso real deve ser acompanhado.
5. **Terminal embutido incompleto** em relação ao texto da tarefa (não segue `cd`, só PowerShell, P4 com terminal aberto não medido).
6. **Busca e pastas de nuvem:** placeholders de nuvem são marcados como link; a busca por pasta não entra em subpastas do OneDrive.
7. **Sem assinatura** até a SignPath; serviço como SYSTEM sem tempo limite para conexões paradas (riscos da Fase 2, sem mudança).

## Recomendação

**Passar para a Fase 4**: os três critérios estão atendidos nos testes e todas as metas se mantêm. Antes de publicar uma versão com o gerenciador padrão, fazer os testes finais dele no PC do usuário (aplicar e reverter acompanhado). A decisão é do usuário.
