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
- [ ] Concluída em: ____ · Resumo:

Seguir a pasta da aba (`cd`) quando ela muda, cmd além do PowerShell, rolar o histórico com a roda, copiar texto, e medir P4 com o terminal aberto.

## T5.5 — Pendências de funcionalidades
- [ ] Concluída em: ____ · Resumo:

- Painel duplo e vista em colunas salvos na sessão.
- Etiquetas na grade e nas colunas; editar nomes de etiquetas; filtro por etiqueta na paleta.
- Busca por pasta entrando em subpastas de nuvem (distinguir placeholder de nuvem de link pela etiqueta de reparse).
- Cores fixas que não passam pelo tema no alto contraste (terminal, etiquetas, realce de sintaxe).
- Ícones do shell nos resultados da busca e na vista em colunas (hoje genéricos).
- Espaço com um filtro digitado entra no filtro; abrir o Quick Look direto quando o filtro deixa um item selecionado.

## T5.6 — Robustez
- [ ] Concluída em: ____ · Resumo:

- `fx-indexd`: tempo limite para conexões paradas.
- Janela de prévia do Windows: atraso de até 250 ms ao mover a janela e sobreposição a menus.
- Benchmarks de entrada conferirem o foco da janela antes de começar (falhas pontuais vistas no portão 3).
- Compilação ARM64 local no WSL (cabeçalhos ARM64 do xwin).
- **Memória parado (P8):** está em 87–88 MB com meta de 88. Descobrir o que carrega WinTrust e Crypt32 no processo parado e o peso do atlas de glifos, e abrir margem.
- `fx-indexd`: a cópia em Program Files não é atualizada pelo atualizador nem removida pelo desinstalador (hoje só avisa).
- A cópia temporária usada para desinstalar fica em `%TEMP%`.

---

## Portão da Fase 5

Lançamento da 1.0: testes manuais feitos, pacote assinado publicado.
