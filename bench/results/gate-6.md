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
| Metas de `PERFORMANCE.md` dentro do limite (revistas em D-064) | P1, P2, P8 e S1 fora do limite; P7 exatamente no limite; P4, QL e paleta (P11) dentro | ❌ |

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
| P2 (b) aba nova | Ctrl+T: mediana 9,9 ms, p95 12,2 | ≤ 16 ms | 11,7 | ✅ |
| P4 tecla → quadro | digitar filtro p95 4,19; setas p95 3,21 (máx. 10,2); caminho p95 4,24 (Enter/Alt+setas 5,36); abas 4,13; paleta 3,29; setas durante reordenação 3,39; busca no disco 3,84 ms | ≤ 8 ms p95, máx. 16 | ≤ 4,38 | ✅ |
| P7 rolagem | 4 perdidos em 3.996 quadros (0,10%), a 239 Hz | ≤ 0,1% | 0,02% | ⚠ no limite |
| Quick Look | JPEG de 9,5 MB, PNG de 10,4 MB, texto: mediana 35,6 ms, p95 43,1 | ≤ 60 ms p95 | 49,1 | ✅ |
| P11 paleta | Ctrl+K e digitação: p95 3,29 ms | ≤ 16 ms | — | ✅ (Configurações e menus próprios sem medição separada) |
| Menu de contexto do shell (T1.16) | 20 menus: mediana 61,7 ms, p95 414,4, máx. 475,7 | ≤ 150 ms p95 | 259 (portão 1) | ❌ neste PC |
| Enumerar 100 mil (informativo) | `FindFirstFileEx` 43,4 ms total, primeiro lote 3,1 ms | ≤ 150 / ≤ 20 ms | — | ✅ |

Os passos que mandam teclas e cliques (`type`, `nav`, `path`, `tabs`, `scroll`, `features`, `menu`) rodaram com o PC livre, a pedido do usuário.

**Menu de contexto:** a mediana continua em ~62 ms. O p95 vem das extensões do shell carregadas na primeira abertura de cada tipo de arquivo, como no portão 1. O fx monta o menu fora da thread de UI e não trava enquanto isso; o custo é do Windows e das extensões instaladas.

**Não medidas hoje:**
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
2. **P7 em 0,10%, exatamente no limite.** 4 quadros perdidos numa das duas sessões, contra 1 no portão 4. Pode ser ruído; vale repetir antes de mexer.
3. **P8 em 88,3 MB contra o limite de 85.** A Fase 6 manteve o fx parado no mesmo patamar, mas o limite novo (D-064) já reprovava antes. É trabalho da T5.7.
4. **S1 em 15,2 MB.** O executável cresceu 0,9 MB com as telas novas:
   - mapa do disco;
   - linha do tempo;
   - duplicados;
   - comparação de pastas;
   - comparação de arquivos;
   - compactados.
5. **P1 em 470 ms contra o limite de 450.** Está estável desde o portão 4, mas o limite novo reprova.
6. **A Fase 5 tem 7 tarefas abertas** (T5.1–T5.7): testes manuais, distribuição, site, terminal, pendências, robustez e as metas novas. O "conferido com o usuário" deste portão depende da T5.1.
7. **O `fx-bench` não acompanhou o D-064.** Os limites impressos são os antigos, o que pode esconder reprovações.
8. **Compactados:**
   - extrair e criar ZIP depende do PowerShell e do .NET do Windows;
   - 7z e RAR só extraem inteiros (D-068);
   - o PowerShell pode estar bloqueado por política em PCs corporativos, e aí o fx só mostra o erro.

## Recomendação

**Corrigir antes de passar.** As funcionalidades da Fase 6 estão completas, mas o portão pede as metas dentro do limite e conferência com o usuário.

Ordem sugerida:
1. Investigar a regressão do P2, que é da própria Fase 6.
2. Atualizar os limites no `fx-bench`.
3. Repetir o P7 para confirmar se o 0,10% é ruído.
4. Seguir a T5.7 (P1, P8, S1) e a T5.1 (conferência com o usuário).

A decisão é do usuário.
