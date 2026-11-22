# Produto e UX

## Para quem
Qualquer pessoa que usa o Explorador do Windows e o acha lento — do usuário comum ao avançado. O padrão precisa parecer familiar a quem vem do Explorador; o poder (teclado, paleta, painel duplo) aparece conforme a pessoa procura.

## Princípios (ordem de prioridade quando entram em conflito)
1. **Nada trava a interface.**
2. **Mostrar algo em até um quadro**: nomes primeiro; tamanhos, ícones e miniaturas chegam depois, sem reordenar a lista sob o cursor.
3. **Tudo se desfaz.**
4. **Familiar antes de esperto.**
5. **Teclado primeiro, mouse igual de bom.**

## Layout
- **Topo:** abas; voltar, avançar e subir; barra de caminho em migalhas (vira campo de texto ao clicar, com autocompletar); grade/lista, itens ocultos e status ("13 itens · lida em 34 ms").
- **Esquerda (recolhível, Ctrl+B):** pasta pessoal, Acesso rápido (fixar e desafixar), unidades com espaço livre.
- **Centro:** lista, grade ou colunas (Ctrl+Shift+3); painel duplo com F3.
- **Sobreposições:** paleta (Ctrl+K), busca (Ctrl+F), Quick Look (Espaço), histórico, renomear em lote, "Sobre" e "Como usar" (F1).
- **Embaixo:** fila de operações e terminal embutido (Ctrl+J).

## Atalhos
Compatíveis com o Explorador, mais os próprios. A lista completa aparece em "Como usar o fx" (F1) e na paleta, ao lado de cada comando.

| Ação | Atalho |
|---|---|
| Paleta de comandos | Ctrl+K (e Ctrl+Shift+P) |
| Como usar o fx | F1 |
| Buscar (Tab: disco inteiro) | Ctrl+F |
| Filtrar a pasta atual | começar a digitar |
| Quick Look | Espaço |
| Renomear | F2 (com vários selecionados: renomear em lote) |
| Painel duplo | F3 (Tab troca, F5 copia, F6 move) |
| Vista em colunas | Ctrl+Shift+3 |
| Editar caminho | Ctrl+L |
| Desfazer / refazer | Ctrl+Z / Ctrl+Y |
| Subir um nível | Alt+↑ |
| Etiquetas | Alt+1…7 (Alt+0 limpa) |
| Terminal embutido / Windows Terminal | Ctrl+J / Ctrl+` |

## Visual
- Fluent 2 do Windows 11, fundo Mica, tema claro/escuro seguindo o sistema; alto contraste com as cores do sistema.
- Fonte do sistema (Segoe UI Variable), tamanhos e espaçamentos em múltiplos de 4 px; uma cor de destaque (teal).
- Transições curtas (≤ 150 ms) que nunca atrasam a ação; nada anima com "efeitos de animação" desligados no Windows.
- Densidade: compacta (24 px por linha), padrão (32 px), confortável (40 px).
- O padrão visual detalhado e o checklist de conferência estão na skill `fx-design` (`.claude/skills/fx-design/SKILL.md`).

## Acessibilidade e idiomas
- Narrador (UI Automation), alto contraste, navegação 100% por teclado, foco sempre visível, DPI por monitor.
- Português e inglês, pelo idioma do Windows ou pela paleta. Cada texto é escrito em par no ponto de uso (`tr!("Copiar", "Copy")`), e um teste varre o código atrás de textos sem tradução (D-053).

## Voz
- Frases curtas, sem ponto de exclamação, sem culpar o usuário; um erro diz o que houve e o próximo passo.
- Verbo no infinitivo nos comandos ("Copiar caminho"), substantivo nos títulos.
- Slogan: "Pastas abrem na hora. Arquivos aparecem enquanto você digita." (D-059)
