# Portão da Fase 6 — relatório

**Data:** 2026-11-23 · **Commit:** 351016a · **Build:** release, compilado no WSL com `cargo xwin`, executado no Windows.

## Máquina

A mesma das fases anteriores: AMD Ryzen 7 5700X, 32 GB, RTX 5070, monitor 1920×1080 a 239 Hz (escala 100%), Kingston NV2 NVMe, Windows 11 25H2. Fixtures em `%TEMP%\fx-fixtures`, cache quente.

## Critério do portão

> Os diferenciais do grupo A entregues e conferidos com o usuário; todas as metas de `docs/PERFORMANCE.md` dentro do limite.

| Critério | Resultado | Passou? |
|---|---|---|
| Tarefas da fase | 15 concluídas, 2 descartadas pelo usuário (T6.15 e juntar PDFs da T6.16); nenhuma aberta | ✅ |
| Grupo A entregue (T6.1–T6.6) | entregue, com testes e conferência por capturas | ✅ |
| Grupo A conferido com o usuário | ainda não: os testes manuais com o usuário são a T5.1, aberta | ❌ |
| Metas de `PERFORMANCE.md` dentro do limite (revistas em D-064) | P1, P2, P8 e S1 fora do limite; P4, P7, QL e P11 não medidas hoje | ❌ |

## Metas (medidas hoje)

Os limites são os de `PERFORMANCE.md` (D-064), mais apertados que os do portão 4. O `fx-bench` ainda imprime os limites antigos (P1 ≤ 500, P2 ≤ 100, P5 ≤ 50, P9 ≤ 150, P8 ≤ 88): por isso diz "pass" em linhas que aqui reprovam.

| Meta | Resultado hoje | Limite | Portão 4 | |
|---|---|---|---|---|
| P1 primeira abertura | mediana 435,7 ms, p95 470,0 | ≤ 450 ms p95 | 473,5 | ❌ |
| P2 pasta de 100 mil, fx carregado | mediana 84,5 ms, p95 99,3 | ≤ 80 ms p95 | 88,1 | ❌ (piorou 13%) |
| P3 ordenar 100 mil | por nome 37,2 ms (p95 37,7); filtro por tecla p95 1,75 ms | ≤ 50 / ≤ 5 ms | 40,6; 0,87 | ✅ |
| P5 busca por tecla | índice sintético de 2 milhões: p95 7,50 ms, máx. 10,3 | ≤ 16 ms p95 | 7,22 | ✅ (sintético) |
| P8 memória parado | mediana 88,3 MB (87,3–88,5, 5 processos) | ≤ 85 MB | 87,8 | ❌ |
| P9 índice por milhão | 71,4 MB (sintético) | ≤ 100 MB | 71,4 | ✅ |
| S1 `fx.exe` | 15,2 MB | ≤ 12 MB | 14,3 (Fase 5) | ❌ (cresceu 6%) |
| Enumerar 100 mil (informativo) | `FindFirstFileEx` 43,4 ms total, primeiro lote 3,1 ms | ≤ 150 / ≤ 20 ms | — | ✅ |

**Não medidas hoje:**
- **P4, P7, Quick Look e P11:** os passos `type`, `nav`, `path`, `tabs`, `scroll`, `features` e `menu` mandam teclas e cliques de verdade e precisam do PC livre.
- **P6 e P5 no índice real do C::** precisam de Administrador.
- **P8b, P8c, P10, P12, P13, S2 e S3:** ainda não são medidas automaticamente pelo `fx-bench`.

## Comparação com Explorador, Files e Everything

Não medida nesta fase. Nenhuma tarefa mexeu nos caminhos já comparados.

## Riscos

1. **P2 piorou de 88 para 99 ms p95.** É a maior regressão da fase. A abertura de pasta ganhou trabalho de T6.9, T6.10 e T6.12:
   - colunas de metadados;
   - agrupamento;
   - status do Git;
   - registro de visitas.

   É preciso conferir se alguma dessas partes entra antes da primeira pintura. Vale perfilar com o `FX_TRACE`.
2. **P8 em 88,3 MB contra o limite de 85.** A Fase 6 manteve o fx parado no mesmo patamar, mas o limite novo (D-064) já reprovava antes. É trabalho da T5.7.
3. **S1 em 15,2 MB.** O executável cresceu 0,9 MB com as telas novas:
   - mapa do disco;
   - linha do tempo;
   - duplicados;
   - comparação de pastas;
   - comparação de arquivos;
   - compactados.
4. **P1 em 470 ms contra o limite de 450.** Está estável desde o portão 4, mas o limite novo reprova.
5. **A Fase 5 tem 7 tarefas abertas** (T5.1–T5.7): testes manuais, distribuição, site, terminal, pendências, robustez e as metas novas. O "conferido com o usuário" deste portão depende da T5.1.
6. **O `fx-bench` não acompanhou o D-064.** Os limites impressos são os antigos, o que pode esconder reprovações.
7. **Compactados:**
   - extrair e criar ZIP depende do PowerShell e do .NET do Windows;
   - 7z e RAR só extraem inteiros (D-068);
   - o PowerShell pode estar bloqueado por política em PCs corporativos, e aí o fx só mostra o erro.

## Recomendação

**Corrigir antes de passar.** As funcionalidades da Fase 6 estão completas, mas o portão pede as metas dentro do limite e conferência com o usuário.

Ordem sugerida:
1. Investigar a regressão do P2, que é da própria Fase 6.
2. Atualizar os limites no `fx-bench`.
3. Rodar com o PC livre os passos de teclado e mouse (P4, P7, QL, P11).
4. Seguir a T5.7 (P1, P8, S1) e a T5.1 (conferência com o usuário).

A decisão é do usuário.
