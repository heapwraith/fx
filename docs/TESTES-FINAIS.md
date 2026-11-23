# Testes finais com o usuário

Testes que precisam de uma pessoa, de outro computador ou de uma conta (tarefa T5.1 da [Fase 5](phases/PHASE-5.md)). O usuário pediu para terminar todas as funcionalidades primeiro e fazer esta rodada no fim. Marque `[x]` com a data e o resultado, e registre o que precisar de correção.

## Fase 2

- [x] **T2.3/T2.11 Narrador:** ligar o Narrador (Ctrl+Win+Enter), navegar pela lista e pelas migalhas; os itens são lidos com nome e tipo. 2026-11-23: árvore do UIA lida pela interface COM (o que o Narrador fala): cada item tem nome, papel "opção", descrição ("pasta", ou tamanho e data) e posição ("2 de 5"); Ctrl+L leva o foco ao campo "Caminho". O Narrador real ligado com o Pathdash navegando por teclado: nada travou. Os arquivos não dizem o tipo por extenso (a extensão está no nome).
- [x] **T2.3/T2.11 Explorador:** Copiar no fx e colar no Explorador (e o contrário); arrastar do fx para o Explorador, a área de trabalho e o navegador (upload), e do Explorador para o fx. **Resultado 2026-11-23:** feito pelo usuário: tudo funcionou.
- [ ] **T2.7 Totais pelo serviço:** com o `fx-indexd` ativo, abrir `C:\` e conferir que os totais das pastas aparecem quase na hora.
- [ ] **T2.9 Visualizadores:** DOCX (com Office instalado), MP4 e MP3 (com um visualizador de mídia) no Quick Look.
- [ ] **T2.10/T4.3 Instalador:** num Windows limpo (ou máquina virtual): instalar pelo `fx-…-setup-x64.exe` (fx-setup) sem administrador, abrir pelo Menu Iniciar, desinstalar por Configurações › Aplicativos e conferir que `%LOCALAPPDATA%\Programs\fx` sumiu; repetir marcando "Apagar também configurações" e conferir que `%APPDATA%\fx` e `%LOCALAPPDATA%\fx` sumiram. SmartScreen: sem assinatura, avisa "editor desconhecido".
- [ ] **T2.11 Assinatura:** inscrição do projeto na SignPath Foundation (só o usuário pode); depois, conferir que o SmartScreen não mostra "editor desconhecido".
- [ ] **T2.11 Beta:** publicar um release no GitHub (os rascunhos `v0.1.0` e `v0.2.0-beta` existentes são antigos: o `v0.1.0` tem um travamento conhecido e não deve ser publicado; gerar um novo pela tag).

## Fase 3

- [x] **T3.1 Gerenciador padrão:** Paleta → "Tornar o fx o gerenciador padrão"; conferir que abrir uma pasta pela área de trabalho, uma unidade em "Este Computador" e o Win+E abrem o fx; depois "Voltar ao Explorador como padrão" e conferir que tudo volta ao Explorador. (Altera `HKCU\Software\Classes`; o reverter restaura o que havia.) **Resultado 2026-11-23:** ativado pela paleta; pasta aberta pelo shell, `C:\`, `D:\` e Win+E abriram no Pathdash, "Este Computador" foi ao Explorador; revertido, as quatro chaves de `HKCU` voltaram iguais ao retrato de antes (comparação com `diff`) e Win+E e `D:\` abriram no Explorador. Achado e corrigido: unidades chegavam como `D:"` (a barra final de `"D:\"` escapa a aspa) e abriam um caminho quebrado.
- [x] **T3.8 alto contraste:** Ligar o alto contraste (Alt+Shift esquerdo+PrtSc) com o fx aberto; em até 1 s ele troca para as cores do sistema, sem Mica; desligar volta ao tema normal. Mover a janela entre monitores com escalas diferentes: texto nítido. **Resultado 2026-11-23 (parcial):** ligado e desligado por `SPI_SETHIGHCONTRAST`; em 1 s o Pathdash passou às cores do sistema (fundo `#202020` do tema "Céu noturno", destaque `#8EE3F0`) e voltou ao tema normal ao desligar. Falta a mudança de escala entre monitores: os dois monitores do PC do usuário estão em 100%.
- [ ] **T3.6 ARM64:** na próxima tag, conferir no release o `fx-X.Y.Z-windows-arm64.zip` e o `setup-arm64.exe`; se houver uma máquina Windows ARM, instalar e abrir.
- [ ] **T3.9 nuvem:** numa pasta do OneDrive, conferir os ícones (nuvem, visto, visto cheio), que a grade não baixa arquivos só na nuvem, e os comandos "Liberar espaço"/"Manter sempre neste dispositivo" na paleta.

## Fase 4

- [ ] **T4.1/T4.4 Atualização real:** com dois releases publicados, abrir a versão antiga: em até um dia (ou por "Atualizar o fx" na paleta) o status diz "Versão X pronta"; fechar o fx e abrir de novo: "Sobre o fx" mostra a versão nova. Repetir com "Atualizar o fx" (reinicia na hora).
- [ ] **T4.3 Instalador por cima de uma instalação feita pelo Inno** (versão 0.2.0-beta antiga): o fx-setup adota a pasta e só uma entrada aparece em Configurações › Aplicativos.
- [ ] **T4.6 Primeira abertura:** num usuário novo do Windows, "Como usar o fx" aparece uma vez e não volta na segunda abertura.
- [x] **T4.5 Instalação no PC do usuário:** 2026-11-22, instalador do CI (commit 50f354f); abriu pelo próprio instalador e mostrou "Como usar".

