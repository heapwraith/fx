---
name: fx-design
description: Padrão de UI/UX do fx (gerenciador de arquivos nativo do Windows em GPUI). Use antes de criar ou mudar qualquer tela, painel, menu, diálogo, estado vazio, texto de interface ou token visual do fx, e para conferir telas na janela (capturas). Base Fluent 2, critérios da Apple HIG e de apps de teclado (Linear, Raycast), com os tokens reais de crates/fx-ui/src/theme.rs e o checklist de conferência.
---

# fx-design

O fx tem que parecer **nativo do Windows 11** e ser **mais rápido e mais calmo que o Explorador**. A base é a Fluent 2; a Apple HIG e os apps de teclado entram como critério de gosto, nunca como aparência (nada de semáforo do macOS, SF Symbols ou vidro do iOS).

## 1. Princípios (em ordem de prioridade)

1. **Velocidade é a estética.** Nada aparece depois do quadro em que a tecla chegou (P4 ≤ 16 ms). Estado de carregamento só se a espera passa de ~150 ms; antes disso, mostrar o que já existe. Animação ≤ 150 ms e nunca bloqueia a próxima tecla.
2. **O conteúdo manda (Apple: deference).** Arquivos são o protagonista; o cromo (barras, bordas, rótulos) é cinza, fino e some. Uma cor de destaque só: o teal do fx.
3. **Clareza antes de densidade (Apple: clarity).** Cada tela responde "onde estou, o que está selecionado, o que posso fazer" sem ler um parágrafo. Hierarquia por peso e cor, não por tamanho de fonte variado.
4. **Teclado primeiro (Linear/Raycast).** Toda ação tem atalho, todo atalho aparece na paleta (`commands.rs`) e em dica. O mouse é um caminho igual, não o único.
5. **Profundidade com propósito (Apple: depth).** Só três planos: base (Mica), camada (lista, painéis) e sobreposição (menus, Quick Look, paleta), cada um com sombra e borda próprias. Nada de sombra decorativa.
6. **Previsível como o Windows.** Atalhos, termos e comportamentos do Explorador onde existirem (F2, Alt+Enter, Ctrl+Shift+N, "Copiar como caminho"). Diferente só quando for claramente melhor, e dizer isso na DECISIONS.md.

## 2. Tokens (fonte da verdade: `crates/fx-ui/src/theme.rs`)

Nunca escrever cor, tamanho ou espaçamento solto no código: usar o token ou criar um em `theme.rs`.

| Token | Claro | Escuro | Uso |
|---|---|---|---|
| `base` | `F3F3F3` | `202020` | fundo da janela, por trás do Mica |
| `layer` | branco 50% | `3A3A3A` 30% | lista, painéis, barra de caminho |
| `text` / `text_secondary` | — / `5C5C5C` | — / `C8C8C8` | primário / rótulos, status, detalhes |
| `accent` | `0B7A75` | `3DD6C6` | foco, indicador do cursor, ícone ativo; nunca fundo grande |
| `selected` | accent 12% | accent 15% | linha ou item selecionado |
| `hover` | preto 4% | branco 6% | hover |
| `divider` | preto 8% | branco 8% | bordas de 1 px |

- **Espaço:** grade de 4 px, `space(n)` = 4·n. Comuns: 4 (dentro de controles), 8, 12 (gap de colunas), 16 (margem de painel).
- **Tipografia:** Segoe UI Variable Text; corpo `BODY_SIZE` 14, legenda `CAPTION_SIZE` 12; peso semibold só para o item atual (última migalha, título de painel). Monoespaçada (Consolas) só para código e caminhos longos em prévia.
- **Raio:** `RADIUS` 4 em controles e linhas; 8 (`RADIUS * 2`) em painéis e sobreposições.
- **Ícones:** Segoe Fluent Icons (constantes `ICON_*`), 12 px nos controles; ícones de arquivo do shell a 16 px na lista.
- **Densidade:** linhas de 24/32/40 px (compacta/padrão/confortável). Todo componente novo tem que caber nas três.
- **Contraste:** texto ≥ 4,5:1 sobre `layer` nos dois temas (há teste em `theme.rs`; acrescente o par novo nele).

## 3. Padrões de componente

- **Linha de lista:** 4 px de margem externa, borda de 1 px (transparente fora do cursor), 12 px de padding interno, indicador de cursor de 3 px à esquerda em accent. Cabeçalhos alinham com isso (16 + 1 px).
- **Sobreposição** (menu, paleta, busca, Quick Look): `deferred(...).with_priority`, borda `divider`, sombra `shadow_lg`, raio 8, fecha com Esc e com clique fora, e devolve o foco a quem tinha. A linha selecionada ocupa a **largura toda** (`w_full`).
- **Texto que pode não caber:** `min_w_0()` + `truncate()` **e** largura vinda do layout (`flex_1`/`w(px)`), senão o GPUI não coloca reticências. Corta-se o menos importante: ponha o que importa no começo da string.
- **Status:** "N itens · seleção · ocultos · lida em X ms". O "lida em" é assinatura do fx: mantenha.
- **Estados vazios e erros:** uma frase que diz o que houve e o próximo passo ("Só há itens ocultos aqui. Ctrl+H mostra."). Português do Brasil, sem ponto de exclamação, sem culpar o usuário.
- **Texto de interface:** verbo no infinitivo nos comandos ("Copiar caminho"), substantivo nos títulos, atalho à direita em `text_secondary`.

## 4. Regras do código que afetam o design

- A thread de UI nunca espera: decodificar, ler disco ou chamar o shell vai para thread própria e a tela mostra o estado intermediário.
- Imagem grande na GPU: liberar com `window.drop_image` quando sair da tela.
- `fx-ui` não usa windows-rs: o que precisar do sistema vai para `fx-shell` com tipo neutro.

## 5. Conferir na janela (obrigatório antes de dizer que a tela está pronta)

Avise o usuário antes: isso manda teclas e cliques de verdade.

1. `cargo xwin build --release -p fx-ui`; abrir o fx em `%TEMP%\fx-fixtures\visual-check` com `FX_NO_STATE=1 FX_SINGLE_INSTANCE=0` (ver `bench/drive.ps1` para os passos `key:`, `type:`, `click:`, `shot:`).
2. Capturar: janela estreita (~900 px) e maximizada; temas claro e escuro; três densidades; lista e grade; o componente aberto e fechado.
3. Abrir cada captura e conferir o checklist:
   - [ ] nada cortado sem reticências; nada sobreposto; nada fora da janela
   - [ ] títulos das colunas alinhados com os valores (meça os pixels)
   - [ ] realce e foco visíveis e na largura certa
   - [ ] nomes iguais em todo lugar (lateral, paleta, migalhas)
   - [ ] contraste ok nos dois temas; ícone sem esticar
   - [ ] o atalho funciona de verdade (Shift+dígito chega como "@" no GPUI: use `fx_shell::shifted_digit`)
   - [ ] diálogo do sistema abriu mesmo (`bench/windows-of.ps1`), não só "retornou Ok"
4. Se a tela tem meta de tempo, medir com `fx-bench` (P4 ≤ 16 ms; Quick Look ≤ 100 ms).
5. Registrar o que achou e corrigiu na DECISIONS.md. Capturas mostram o nome do usuário no caminho: **nunca** fazer commit delas.

## 6. Quando usar outras skills

- Crítica ampla de uma captura: `design:design-critique` com este documento como referência.
- Auditoria de acessibilidade: `design:accessibility-review` (e o Narrador ao vivo com o usuário).
- Textos: `design:ux-copy`, respeitando a seção 3.
