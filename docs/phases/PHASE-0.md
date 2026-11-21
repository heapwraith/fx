# Fase 0 — Prova de velocidade

**Objetivo:** provar, antes de construir qualquer interface de verdade, que a stack escolhida (Rust + GPUI + leitura da MFT) bate as metas de desempenho. Se não bater, trocamos de stack agora, quando é barato.

**Duração estimada:** 4 semanas. **Fora de escopo nesta fase:** visual bonito, abas, menu de contexto, operações de arquivo, configurações, instalador.

Regras: faça as tarefas **na ordem**. Ao concluir uma, marque `[x]`, coloque a data e uma linha de resumo com os números.

---

## T0.1 — Workspace e GPUI rodando no Windows
- [x] Concluída em: 2026-11-21 · Resumo: gpui 0.2.2 com patch (shaders em runtime) compilado do WSL com `cargo xwin`; janela com texto DirectWrite e botão funcionando no Windows; build debug 19,5 s (incremental), release 3 min 09 s (D-007, D-010).

**Fazer**
1. Confirmar que `rustc --version` mostra 1.100.0 e o alvo `x86_64-pc-windows-msvc`.
2. Os crates já existem como esqueleto em `crates/` e compilam; confirme com `cargo build` no Windows.
3. Escolher a distribuição do GPUI. Ordem de preferência:
   1. o crate oficial `gpui` do crates.io (0.2.x), com o `gpui_platform` correspondente, se existir para a versão;
   2. se não compilar ou não abrir janela no Windows, testar um fork mantido (`gpui-ce`, `gpui-box`) e registrar o motivo;
   3. se nenhum funcionar, parar e reportar ao usuário (isso aciona o plano B).
4. Abrir uma janela com um texto e um botão que responde a clique.

**Aceite**
- `cargo run -p fx-ui` abre uma janela nativa no Windows, o texto aparece nítido (DirectWrite) e o clique muda o texto.
- A versão exata do GPUI está fixada (versão exata no `Cargo.toml` ou `rev` do git) e registrada em `docs/DECISIONS.md` (D-007).
- `cargo clippy --workspace --all-targets -- -D warnings` passa.

---

## T0.2 — Gerador de fixtures
- [x] Concluída em: 2026-11-21 · Resumo: `fx-bench fixtures` gera as 4 pastas (161.100 arquivos) em 37–40 s (release); 2ª execução 0,00 s; apagar e regenerar dá listagem idêntica (sha256); `--clean` protegido por guarda com testes.

**Fazer**
- `cargo run -p fx-bench --release -- fixtures` cria as pastas descritas em `docs/PERFORMANCE.md` dentro de `%TEMP%\fx-fixtures`.
- `... -- fixtures --clean` apaga **somente** `%TEMP%\fx-fixtures`.

**Aceite**
- Geração determinística (semente fixa): rodar duas vezes produz os mesmos nomes.
- Idempotente: se a pasta já existe e está completa, não recria.
- O código recusa apagar qualquer caminho que não termine em `fx-fixtures` (teste unitário cobrindo isso).

---

## T0.3 — Enumeração de pastas mais rápida possível (fx-fs)
- [x] Concluída em: 2026-11-21 · Resumo: vencedora `FindFirstFileExW`: na `flat-100k`, 1º lote 2,8 ms (p95 3,7) e total 49 ms (p95 58) contra metas de 20/150 ms; `read_dir` 52 ms, `FileIdBothDirInfo` 89 ms (D-009).

**Fazer**
1. Implementar três estratégias atrás da mesma interface:
   - `std::fs::read_dir` (linha de base),
   - `FindFirstFileExW` com `FindExInfoBasic` e `FIND_FIRST_EX_LARGE_FETCH`,
   - `GetFileInformationByHandleEx` com `FileIdBothDirectoryInfo` (leitura em blocos grandes).
2. Cada estratégia entrega `Entry { name, size, modified, attributes }` em **lotes** por canal.
3. Benchmark `fx-bench enum` na `flat-100k`: tempo até o primeiro lote e até o fim, mediana e p95 de 10 execuções.

**Aceite**
- Resultados gravados em `bench/results/` e a estratégia vencedora registrada em `docs/DECISIONS.md` com os números.
- Primeiro lote em ≤ 20 ms e pasta completa em ≤ 150 ms na `flat-100k` (máquina do usuário, release).
- Testes com `tempfile` cobrindo: pasta vazia, nomes Unicode, caminho > 260 caracteres, arquivo sem permissão de leitura.

---

## T0.4 — Armazenamento da lista, ordenação natural e filtro (fx-core)
- [x] Concluída em: 2026-11-21 · Resumo: `FileList` em colunas com ordenação natural (sem acento/maiúscula, pastas primeiro) e filtro; 100k: ordenar por nome 35,6 ms (p95 35,8; meta 50), filtrar por tecla 0,95 ms (p95 1,2; meta 5).

**Fazer**
- Estrutura em colunas (ver ARCHITECTURE.md, "Desenho da lista"): buffer único de nomes + vetores de tamanho/data + vetor de índices para ordenação.
- Ordenação por nome (natural, sem diferenciar maiúsculas, como o Explorador), tamanho, data e tipo; pastas primeiro.
- Filtro local "digite para filtrar" (contém, sem acento/maiúscula).

**Aceite**
- Testes unitários: "a2" antes de "a10", acentos, maiúsculas, empates estáveis, pastas primeiro.
- Benchmark: ordenar 100 mil entradas por nome ≤ 50 ms; filtrar 100 mil por tecla ≤ 5 ms.
- `fx-core` continua sem dependência de `windows` e com `#![forbid(unsafe_code)]`.

---

## T0.5 — Lista virtualizada no GPUI (fx-ui)
- [x] Concluída em: 2026-11-21 · Resumo: lista virtualizada (≤ 26 linhas construídas por quadro). **P2 FALHA desde a partida do processo: 406 ms** (25 ms com o app aberto; D-013). P7 a 239 Hz: p95 4,38 ms contra meta de 4,2 ms, falha pela letra por jitter do vsync, com 1 quadro perdido em 3.425 (máx. 6,3 ms). Janela responde durante a carga de 1M arquivos via `\\wsl.localhost`: máx. 2,9 ms.

**Fazer**
- `fx <caminho>` abre uma janela com a lista da pasta: colunas Nome, Tamanho, Data. Só as linhas visíveis são construídas.
- A enumeração (T0.3) roda fora da UI; os lotes vão sendo anexados sem travar a rolagem.
- Instrumentação: com `FX_TRACE=1`, o app registra em `bench/results/` o tempo desde o início do processo até o **primeiro quadro com linhas** e o tempo de cada quadro durante a rolagem.

**Aceite**
- **P2:** primeira pintura da `flat-100k` ≤ 100 ms (mediana de 10 aberturas).
- **P7:** rolar do início ao fim com a roda e arrastando a barra sem quadros acima de 8,3 ms (p95) num monitor de 120 Hz, ou acima de 16,6 ms num de 60 Hz (anotar qual).
- A janela continua respondendo (dá para mover e redimensionar) enquanto uma pasta de rede lenta ou enorme carrega.

---

## T0.6 — Latência de input
- [x] Concluída em: 2026-11-21 · Resumo: digitar para filtrar na `flat-100k`; **P4: p95 3,4 ms** (mediana 2,7, máx. 3,4; meta 16) em 33 teclas reais (3× "report-123" + Esc), medido do evento de tecla até depois do present do quadro.

**Fazer**
- Digitar para filtrar na `flat-100k` usando o filtro da T0.4.
- Medir input → quadro atualizado (marca de tempo no evento de tecla e no fim do quadro).

**Aceite**
- **P4:** ≤ 16 ms no p95 ao digitar 10 caracteres seguidos.

---

## T0.7 — Leitura da MFT e índice em memória (fx-index)
- [ ] Concluída em: ____ · Resumo: ____
- **Status 2026-11-21:** implementada, mas **falta a medição elevada**. Índice em colunas (`fx_index::Index`), leitura paralela da MFT (`read_volume`) e diário USN (`JournalWatcher`). Sem admin, o comando falha com "execute como Administrador" (testado). No sintético de 2 milhões de arquivos: montagem em 0,31 s e **71 MB por milhão** (P9 ≤ 150). Falta rodar num PowerShell **como Administrador**, na pasta do repositório (`\\wsl.localhost\Ubuntu\home\<usuario>\projetos\fx`):
  1. `.\target\x86_64-pc-windows-msvc\release\fx-bench.exe index --volume C:` (P6 e P9 reais);
  2. o teste USN (criar, renomear e apagar um arquivo em `%TEMP%`): depois de `cargo xwin test --target x86_64-pc-windows-msvc -p fx-index --no-run`, rodar `& (Get-Item .\target\x86_64-pc-windows-msvc\debug\deps\fx_index-*.exe | Select-Object -First 1) --ignored`.

**Fazer**
- Com `ntfs-reader`, ler a MFT do volume C: e montar um índice em memória: nome compactado, id do registro, id do pai, flag de pasta, tamanho, data.
- Reconstruir o caminho completo sob demanda (subindo pelos pais), não guardar o caminho de cada arquivo.
- Comando `fx-bench index --volume C:` (exige terminal elevado) imprime: arquivos indexados, tempo, memória usada.

**Aceite**
- **P6:** índice de ~2 milhões de arquivos em ≤ 10 s (anotar o número real de arquivos do disco).
- **P9:** ≤ 150 MB por milhão de arquivos.
- Testes que precisam de admin marcados `#[ignore]`; sem admin, o comando falha com mensagem clara ("execute como Administrador"), sem pânico.
- Ler o diário USN depois da montagem e aplicar criação/renomeação/exclusão de um arquivo de teste criado em `%TEMP%` (teste `#[ignore]`).

---

## T0.8 — Busca difusa por tecla
- [ ] Concluída em: ____ · Resumo: ____

**Fazer**
- Busca por nome sobre o índice da T0.7: correspondência por subsequência com pontuação (estilo fzf/nucleo), sem diferenciar maiúsculas e acentos, com resultados parciais (primeiros 200) devolvidos antes do fim.
- `fx-bench search` interativo: a cada tecla, imprime o tempo e os 10 primeiros resultados.

**Aceite**
- **P5:** ≤ 50 ms por tecla no p95, com o índice inteiro do C:, para consultas de 1 a 12 caracteres (lista de consultas fixa em `fx-bench`).
- Busca usa todos os núcleos e pode ser cancelada quando chega a próxima tecla.

---

## T0.9 — Acessibilidade no protótipo
- [ ] Concluída em: ____ · Resumo: ____

**Fazer**
- Verificar no código do GPUI fixado se há suporte a UI Automation (o que o Narrador usa) no Windows.
- Testar o protótipo com o Narrador (Ctrl+Win+Enter): ele lê a janela, a lista e o item selecionado?

**Aceite**
- Registro em `docs/DECISIONS.md` (D-008) com o resultado: "funciona", "parcial" (o que falta) ou "não funciona", e o caminho proposto (contribuir com o upstream, camada própria de UI Automation ou plano B).

---

## T0.10 — Comparação e relatório do portão
- [ ] Concluída em: ____ · Resumo: ____

**Fazer**
- Medir as mesmas pastas no Explorador, no Files e no Everything (busca), na mesma máquina, seguindo `docs/PERFORMANCE.md`. O usuário pode precisar fazer as medições manuais em vídeo; prepare o roteiro para ele.
- Rodar `/gate`.

**Aceite**
- `bench/results/gate-0.md` com a tabela meta × resultado × passou e a comparação com os concorrentes.
- Seção "Benchmarks" do `README.md` atualizada com os números.

---

## Portão da Fase 0

Passa para a Fase 1 somente se:
1. **P2, P4, P5 e P7** passaram.
2. **P6 e P9** passaram ou têm plano claro de correção.
3. A acessibilidade (T0.9) tem caminho viável.

Se P2, P4 ou P7 falharem por causa do GPUI (e não do nosso código), a recomendação é o **plano B**: renderizador próprio em Direct3D 11 + DirectWrite. Pare e apresente os números ao usuário — a decisão é dele.
