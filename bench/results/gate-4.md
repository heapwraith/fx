# Portão da Fase 4 — relatório

**Data:** 2026-11-22 · **Versão:** 0.2.0-beta (commit f383f66) · **Build:** release, compilado no WSL com `cargo xwin`, executado no Windows; CI no GitHub (`check` e `arm64`, agora gerando os instaladores com o `fx-setup`).

## Máquina

A mesma das fases anteriores: AMD Ryzen 7 5700X, 32 GB, RTX 5070, monitor 1920×1080 a 239 Hz (escala 100%), Kingston NV2 NVMe, Windows 11 25H2 (exibição em inglês). Fixtures em `%TEMP%\fx-fixtures`, cache quente.

## Tarefas

| Tarefa | Estado |
|---|---|
| T4.1 Atualizador | ✅ consulta ao GitHub, SHA-256, assinatura quando houver |
| T4.2 Polimento | ✅ transições ≤ 150 ms, estados vazios, erros nos dois idiomas |
| T4.3 Instalador próprio (fx-setup) | ✅ substitui o Inno Setup |
| T4.4 Atualização ao fechar (como o Zed) | ✅ |
| T4.6 Telas "Sobre" e "Como usar" | ✅ (pedido do usuário) |
| T4.5 Este relatório e instalação no PC do usuário | ✅ |

## Critérios do portão

| Critério | Resultado | Passou? |
|---|---|---|
| **1. Atualizador testado** (versão, hash, assinatura quando houver) e verificado contra o GitHub | 9 testes da lógica (versões, JSON, escolha do instalador, hash certo/alterado/ausente, uma vez por dia), 3 do Windows (SHA-256, só HTTPS, assinatura); consulta real à API do GitHub; ponta a ponta numa pasta de teste: 0.2.0 → 9.9.9 ao fechar o fx e 9.9.9 → 9.9.10 pela paleta, com o fx reaberto sozinho | ✅ |
| **2. P1–P9 mantidas** | todas dentro da meta (tabela abaixo); P8 no limite | ✅ (P8 justo) |
| **3. fx instalado e em uso no PC do usuário** | instalado pelo `fx-setup` (ver o fim) | ✅ |

## Metas (medidas hoje)

| Meta | Resultado | Meta | Portão 3 | |
|---|---|---|---|---|
| P1 primeira abertura | mediana 430,4 ms, p95 473,5 | ≤ 500 ms | 420,9 | ✅ |
| P2 pasta de 100 mil, fx carregado | do pedido 71,8 / 88,1 ms; aba nova 9,9 / 11,7 ms | ≤ 100 ms | 68,9; 11,0 | ✅ |
| P3 ordenar 100 mil | por nome 38,5 ms (p95 40,6); filtro por tecla 0,87 ms | ≤ 50 / ≤ 5 ms | 39,4 | ✅ |
| P4 tecla → quadro | paleta p95 3,37; setas 3,38; busca 3,51; abas 4,38 ms | ≤ 16 ms | ≤ 3,95 | ✅ |
| P5 busca | índice sintético de 2 milhões: p95 7,22 ms | ≤ 50 ms | 7,84 | ✅ |
| P7 rolagem | 1 perdido em 4.001 quadros (0,02%) | ≤ 0,5% | 0 | ✅ |
| P8 memória parado | medianas 87,9 / 87,3 / 87,7 / 88,1 (antes do ajuste); 87,8 / 87,2 / 87,2 depois | ≤ 88 MB | 86,9 | ⚠ no limite |
| P9 índice por milhão | 71,4 MB (sintético) | ≤ 150 MB | 71,4 | ✅ |
| Quick Look | p95 49,1 ms | ≤ 100 ms | 47,4 | ✅ |

**P8:** a Fase 4 somou cerca de 0,5 MB ao fx parado. Uma parte vinha do atualizador: o `fx.exe` passava a carregar WinHTTP, WinTrust e BCrypt já na abertura. Elas agora são carregadas só quando uma verificação de atualização roda (*delay-load*), e a WinHTTP saiu da abertura. A WinTrust, a Crypt32 e a BCrypt continuam aparecendo no processo parado, carregadas por outra parte (o shell do Windows, ao pedir ícones). Em 7 rodadas, uma passou do limite (88,1). É a meta mais apertada do projeto.

## Riscos

1. **P8 no limite** (88 MB, D-038): qualquer acréscimo pede compensação. Candidatos a investigar na Fase 5: o que carrega WinTrust e Crypt32 no processo parado, e o atlas de glifos.
2. **Instalador maior:** 15,2 MB contra 5,8 MB do Inno, porque o programa do instalador leva o GPUI sem compressão.
3. **Sem assinatura** até a SignPath: o SmartScreen avisa "editor desconhecido", e o atualizador confia no HTTPS do GitHub e no `SHA256SUMS.txt` do mesmo release.
4. **Serviço de índice:** a cópia do `fx-indexd` em Program Files não é atualizada pelo atualizador nem removida pelo desinstalador (só avisa).
5. **Atualização automática:** baixa sozinha quando ligada (padrão, como no Zed); a primeira atualização real só acontece quando houver um release publicado.
6. **Testes manuais** pendentes, agora na Fase 5 (`docs/TESTES-FINAIS.md`).

## Recomendação

**Passar para a Fase 5** (pendências de todas as fases), com o fx em uso diário no PC do usuário. A decisão é do usuário.
