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
- **Topo:** abas; barra de caminho em migalhas (vira campo de texto ao clicar, com autocompletar); botões voltar/avançar/subir; busca.
- **Esquerda (recolhível):** Acesso rápido, unidades, nuvem, tags.
- **Centro:** lista, grade ou colunas; painel duplo com F3.
- **Direita (opcional):** inspetor com pré-visualização e metadados.
- **Rodapé:** seleção atual, espaço livre, fila de operações.

## Atalhos (compatíveis com o Explorador, mais os próprios)
| Ação | Atalho |
|---|---|
| Nova aba / fechar aba | Ctrl+T / Ctrl+W |
| Paleta de comandos | Ctrl+K (e Ctrl+Shift+P) |
| Buscar no disco | Ctrl+F |
| Filtrar a pasta atual | começar a digitar |
| Quick Look | Espaço |
| Renomear | F2 (com vários selecionados: renomear em lote) |
| Painel duplo | F3 |
| Editar caminho | Ctrl+L ou Alt+D |
| Desfazer / refazer | Ctrl+Z / Ctrl+Y |
| Subir um nível | Alt+↑ |
| Abrir terminal aqui | Ctrl+` |

## Visual
- Fluent 2 do Windows 11, fundo Mica, tema claro/escuro seguindo o sistema.
- Fonte do sistema (Segoe UI Variable), tamanhos e espaçamentos em múltiplos de 4 px.
- Animações curtas (≤ 150 ms) que nunca atrasam a ação; respeitar "efeitos de animação" desligados no Windows.
- Densidade: compacta (24 px por linha), padrão (32 px), confortável (40 px).

## Acessibilidade e idiomas
- Narrador (UI Automation), alto contraste, navegação 100% por teclado, foco sempre visível, DPI por monitor.
- Português e inglês a partir da beta pública; textos em arquivos de recurso, nunca no código.
