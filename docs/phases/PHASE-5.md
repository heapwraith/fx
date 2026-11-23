# Fase 5 — Pendências de todas as fases e lançamento da 1.0

> Criada em 2026-11-22, a pedido do usuário: tudo o que faltou nas fases anteriores fica numa etapa só, separada para o final. A Fase 4 terminou com o fx instalado e em uso no PC do usuário (`bench/results/gate-4.md`). **Ainda não iniciada:** começa quando o usuário pedir. A ordem abaixo é uma sugestão; o usuário escolhe.

**Objetivo:** fechar tudo o que ficou para depois, testar com o usuário o que só uma pessoa pode testar e publicar a 1.0 assinada.

## T5.1 — Testes manuais com o usuário
- [ ] Concluída em: ____ · Resumo:

Toda a lista de `docs/TESTES-FINAIS.md` (Narrador, Explorador, totais pelo serviço, visualizadores de Office/mídia, instalador num Windows limpo, gerenciador padrão aplicado e revertido de verdade, alto contraste e DPI, OneDrive real, ARM64) e as correções que saírem dela.

## T5.2 — Distribuição
- [ ] Concluída em: ____ · Resumo:

- Assinatura pela SignPath Foundation (inscrição é do usuário; passos em `docs/SIGNING.md`).
- Publicar a beta e depois a 1.0 no GitHub (o atualizador só enxerga releases publicados, não rascunhos).
- **Licenças de terceiros:** gerar um `THIRD-PARTY-NOTICES` com a licença de cada dependência (GPUI, Apache 2.0, e as demais) e incluí-lo no instalador e no zip; a Apache 2.0 pede que a licença acompanhe o programa e que arquivos modificados sejam indicados.
- Pacote MSIX para a Microsoft Store e manifesto do winget.
- Ações do CI que ainda usam Node 20.
- Instalador menor (hoje 15,2 MB, porque o programa do instalador leva o GPUI sem compressão).

## T5.3 — Site, documentação de uso e vídeo
- [ ] Concluída em: ____ · Resumo:

Site do projeto, guia de uso (atalhos, busca, painel duplo, etiquetas, terminal, gerenciador padrão) e vídeo de 30 s lado a lado com o Explorador.

## T5.4 — Terminal embutido completo (T3.5)
- [x] Concluída em: 2026-11-23 · Resumo: o terminal acompanha a pasta da aba quando o shell espera num prompt vazio (`Set-Location -LiteralPath` ou `cd /d`, nunca no meio do que se digita); "Terminal embutido: Prompt de Comando" na paleta abre o cmd (Ctrl+J reabre o último usado); a roda rola o histórico (até 2 mil linhas, o cursor some e a legenda diz quantas linhas acima); arrastar seleciona texto, Ctrl+Shift+C (ou Ctrl+C com seleção) copia, Ctrl+Shift+V cola; alto contraste com as cores do sistema. P4 digitando no terminal (`fx-bench terminal`): p95 4,55 ms, máximo 4,59 ms. Conferido de ponta a ponta com teclado e mouse (cópia de "ola-fx", `cd` ao abrir uma pasta).

Seguir a pasta da aba (`cd`) quando ela muda, cmd além do PowerShell, rolar o histórico com a roda, copiar texto, e medir P4 com o terminal aberto.

## T5.5 — Pendências de funcionalidades
- [x] Concluída em: 2026-11-23 · Resumo: painéis extras (caminho e vista em colunas de cada um) e a vista em colunas da aba salvos na sessão (tabelas `tab_layout` e `tab_panes`; conferido fechando e reabrindo); etiquetas na grade e nas colunas, nomes próprios para etiquetas ("Renomear etiqueta" na paleta, guardados no `tags.db`, valem em `etiqueta:`) e "Etiqueta: …" na paleta abrindo a busca por etiqueta; a busca "nesta pasta" e os totais entram em pastas da nuvem (o tipo do ponto de reparse decide: só links, junções e pontos de montagem ficam de fora); terminal, botão de perigo e etiquetas com as cores do sistema no alto contraste; ícones do shell nos resultados da busca e na vista em colunas; com o filtro deixando um item só, Espaço abre o Quick Look dele.

- Painel duplo e vista em colunas salvos na sessão.
- Etiquetas na grade e nas colunas; editar nomes de etiquetas; filtro por etiqueta na paleta.
- Busca por pasta entrando em subpastas de nuvem (distinguir placeholder de nuvem de link pela etiqueta de reparse).
- Cores fixas que não passam pelo tema no alto contraste (terminal, etiquetas, realce de sintaxe).
- Ícones do shell nos resultados da busca e na vista em colunas (hoje genéricos).
- Espaço com um filtro digitado entra no filtro; abrir o Quick Look direto quando o filtro deixa um item selecionado.

## T5.6 — Robustez
- [ ] Concluída em: ____ · Resumo:
- Parcial em 2026-11-23: o `fx-indexd` desliga um cliente que não termina a conversa em 15 s (um vigia por conexão cancela a leitura ou escrita presa com `CancelSynchronousIo`; desconectar de outra thread esperaria a mesma chamada presa); o teste do pipe confere que um cliente parado é desligado e não atrapalha os outros. Os scripts de entrada (rolagem, menu, `drive.ps1`) conferem que o fx está em primeiro plano antes de mandar teclas ou cliques e falham com mensagem clara se não estiver (os de digitação já faziam isso); o `drive.ps1` refoca antes de cada passo de entrada. Rolagem medida depois: 0 de 4.001 quadros perdidos. A cópia do desinstalador em `%TEMP%` se apaga ao terminar (um `cmd` oculto espera ~2 s e a remove) e todo `fx-setup` que roda apaga cópias velhas; conferido desinstalando de uma pasta de teste: a pasta sumiu e nenhuma cópia ficou. ARM64 no WSL: só o `fx-indexd` compila; o `ring` saiu (D-071), mas o `psm` não monta com `clang-cl` fora do Windows (diagnóstico na D-071); o CI segue compilando o ARM64.

- `fx-indexd`: tempo limite para conexões paradas.
- Janela de prévia do Windows: atraso de até 250 ms ao mover a janela e sobreposição a menus.
- Benchmarks de entrada conferirem o foco da janela antes de começar (falhas pontuais vistas no portão 3).
- Compilação ARM64 local no WSL (cabeçalhos ARM64 do xwin).
- **Memória parado (P8):** está em 87–88 MB com meta de 88. Descobrir o que carrega WinTrust e Crypt32 no processo parado e o peso do atlas de glifos, e abrir margem.
- `fx-indexd`: a cópia em Program Files não é atualizada pelo atualizador nem removida pelo desinstalador (hoje só avisa).
- A cópia temporária usada para desinstalar fica em `%TEMP%`.

## T5.7 — Metas novas que hoje reprovam (D-064)
- [ ] Concluída em: ____ · Resumo:
- Parcial em 2026-11-23: S1 subiu para ≤ 20 MB (D-069, pedido do usuário; hoje 18,4 MB). `gpui_http_client` vendorizado sem o `reqwest` (D-071): 56 pacotes a menos, mas sem efeito no tamanho nem na memória (o código já era descartado). P2 voltou a 73–78 ms de mediana (D-070). P8 investigado (2026-11-23): 88,0 MB parado; sem ícones (`FX_ICONS=0`) 88,4, sem Mica 88,4, sem estado 88,2 (o teste já roda assim): nenhuma dessas partes pesa. Um app GPUI simples (o `fx-setup` parado) fica em 73 MB, então o fx soma ~15 MB. Ele tem 51 threads contra 34 e carrega 36 DLLs que o `fx-setup` não carrega, várias do shell e de extensões de terceiros (`windows.system.launcher`, `staterepository*`, `thumbcache`, `urlmon`, `propsys`, `nvui` da NVIDIA, `ntshrui`, `cscapi`, `wlidprov`, `idstore`, `uiautomationcore`, `oleacc`). Próximo passo: descobrir com um perfil de carga de DLLs (WPA ou Process Monitor) o que as puxa na partida e adiar essa parte para quando for usada.

- **P8 ≤ 85 MB e S1 `fx.exe` ≤ 12 MB:** tirar do GPUI vendorizado o cliente HTTP que o fx não usa (`gpui_http_client`, `reqwest`, `rustls`, `ring`) e medir o que mais pesa no processo parado (WinTrust/Crypt32 carregadas pelo shell, atlas de glifos).
- **S2 instalador ≤ 10 MB:** comprimir o programa do instalador (hoje 6,7 MB sem compressão) ou reduzir o GPUI que ele leva.
- **P6 índice de 2 milhões ≤ 10 s:** ler os volumes em paralelo, reduzir alocações e medir num disco real com ~2 milhões de arquivos.
- **Medições que faltam no `fx-bench`:** P8b (aba a mais), P8c (janela a mais), P10 (cópia contra o Explorador), P13 (instalar e atualizar), S1–S3 (tamanhos).

---

## Portão da Fase 5

Lançamento da 1.0: testes manuais feitos, pacote assinado publicado e todas as metas de `docs/PERFORMANCE.md` (D-064) dentro do limite.
