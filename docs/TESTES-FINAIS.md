# Testes finais com o usuário

O usuário pediu para terminar todas as funcionalidades primeiro (2026-11-22) e só depois fazer os testes manuais e as correções. Esta é a lista do que ficou para essa rodada, por tarefa. Marque `[x]` com a data e o resultado.

## Fase 2

- [ ] **T2.3/T2.11 Narrador:** ligar o Narrador (Ctrl+Win+Enter), navegar pela lista e pelas migalhas; os itens são lidos com nome e tipo.
- [ ] **T2.3/T2.11 Explorador:** copiar no fx e colar no Explorador (e o contrário); arrastar do fx para o Explorador, a área de trabalho e o navegador (upload), e do Explorador para o fx.
- [ ] **T2.7 Totais pelo serviço:** com o `fx-indexd` ativo, abrir `C:\` e conferir que os totais das pastas aparecem quase na hora.
- [ ] **T2.9 Visualizadores:** DOCX (com Office instalado), MP4 e MP3 (com um visualizador de mídia) no Quick Look.
- [ ] **T2.10/T4.3 Instalador:** num Windows limpo (ou máquina virtual): instalar pelo `fx-…-setup-x64.exe` (fx-setup) sem administrador, abrir pelo Menu Iniciar, desinstalar por Configurações › Aplicativos e conferir que `%LOCALAPPDATA%\Programs\fx` sumiu; repetir marcando "Apagar também configurações" e conferir que `%APPDATA%\fx` e `%LOCALAPPDATA%\fx` sumiram. SmartScreen: sem assinatura, avisa "editor desconhecido".
- [ ] **T2.11 Assinatura:** inscrição do projeto na SignPath Foundation (só o usuário pode); depois, conferir que o SmartScreen não mostra "editor desconhecido".
- [ ] **T2.11 Beta:** publicar o rascunho do release `v0.2.0-beta` no GitHub.

## Fase 3

- [ ] **T3.1 Gerenciador padrão:** paleta → "Tornar o fx o gerenciador de arquivos padrão"; conferir que abrir uma pasta pela área de trabalho, uma unidade em "Este Computador" e o Win+E abrem o fx; depois "Voltar ao Explorador como gerenciador padrão" e conferir que tudo volta ao Explorador. (Altera `HKCU\Software\Classes`; o reverter restaura o que havia.)
- [ ] **T3.8 alto contraste:** ligar o alto contraste (Alt+Shift esquerdo+PrtSc) com o fx aberto; em até 1 s ele troca para as cores do sistema, sem Mica; desligar volta ao tema normal. Mover a janela entre monitores com escalas diferentes: texto nítido.
- [ ] **T3.6 ARM64:** na próxima tag, conferir no release o `fx-X.Y.Z-windows-arm64.zip` e o `setup-arm64.exe`; se houver uma máquina Windows ARM, instalar e abrir.
- [ ] **T3.9 nuvem:** numa pasta do OneDrive, conferir os ícones (nuvem, visto, visto cheio), que a grade não baixa arquivos só na nuvem, e os comandos "Liberar espaço"/"Manter sempre neste dispositivo" na paleta.
