# Segurança de dados e limites de autonomia

Um gerenciador de arquivos pode destruir o trabalho de alguém em um clique. Estas regras valem para o código **e** para o agente que o escreve.

## 1. Limites do agente (o que exige perguntar antes)

Pergunte ao usuário e espere a resposta antes de:

- **Alterar o registro do Windows** (`reg add`, `reg delete`, `Set-ItemProperty HKCU:/HKLM:`, ou código que faça isso fora de um teste com chave temporária). Isso inclui registrar o app como gerenciador padrão (só quando o usuário pedir) e instalar o fx no PC do usuário.
- **Rodar qualquer coisa como Administrador** ou instalar serviços (`sc create`, `New-Service`).
- **Apagar, mover ou renomear arquivos fora de**: pastas criadas por `tempfile`, `%TEMP%\fx-fixtures`, `target/` e o próprio repositório.
- Publicar crates, `git push --force`, mexer em configurações globais do git.
- **Mudar uma decisão** registrada em `docs/DECISIONS.md`.

Liberado sem perguntar (pedido do usuário em 2026-11-23): `git push` comum para o `main` depois dos testes verdes, e adicionar ou atualizar dependências que sigam a D-069 (rodam no Windows e no Linux), com versão exata em `[workspace.dependencies]` e o motivo na `DECISIONS.md`.

Releases (D-075, escolha do usuário em 2026-11-23): criar a tag `vX.Y.Z` está liberado quando o usuário pedir ou ao fechar um conjunto de mudanças, com a versão do `Cargo.toml` e a seção do `CHANGELOG.md` atualizadas e o CI do commit verde. O CI publica o release sozinho (pré-lançamento enquanto for 0.x), só depois de testes, compilação ARM64 e auditoria passarem; o release chega a todo Pathdash instalado pela atualização automática. Nunca apagar nem republicar um release já publicado: um problema se corrige com uma versão nova.

Nunca, nem com pedido: formatar unidades, desativar o Windows Defender, apagar pontos de restauração, tocar em `C:\Windows`, `C:\Program Files` ou nas pastas pessoais reais do usuário (Documentos, Desktop, Fotos etc.) em testes.

## 2. Regras do motor de operações (código)

1. **Excluir = Lixeira por padrão.** Exclusão permanente só com ação explícita (Shift+Delete) e confirmação.
2. **Copiar é verificado:** ao terminar, compara o tamanho; em modo "verificar", compara também um hash. Mover entre unidades = copiar, verificar, só então apagar a origem.
3. **Nunca sobrescrever sem decisão.** Conflito de nome sempre gera um conflito na fila; o padrão é "manter os dois" (renomear com sufixo), nunca "substituir".
4. **Toda operação concluída entra no histórico de desfazer** com o que é preciso para revertê-la (caminhos originais, nomes antigos, itens enviados à Lixeira).
5. **Falha no meio não deixa lixo nem perde dado:** cópia escreve em arquivo temporário no destino e renomeia no fim; se cancelar ou falhar, o temporário é removido e a origem fica intacta.
6. **Operações longas são retomáveis e canceláveis** sem corromper arquivos.
7. Caminhos sempre canonizados antes de operar; recusar operar em caminhos de sistema (`C:\Windows`, raiz de unidade como destino de exclusão) sem confirmação extra.

## 3. Testes obrigatórios do motor de operações

`fx-ops` tem testes (desde o portão da Fase 2) que simulam:
- falta de espaço no meio da cópia,
- arquivo em uso / acesso negado,
- cancelamento no meio,
- queda (processo morto) no meio — ao reabrir, nada corrompido,
- nomes Unicode, caminhos > 260 caracteres, arquivos de 0 bytes e > 4 GB.

Nenhum desses pode resultar em perda de dados. Um único caso de perda reprova qualquer portão.

## 4. Instalador e atualizações

- O instalador troca os arquivos de uma vez e volta ao que havia se algo falhar; nunca apaga um arquivo que não instalou.
- Uma atualização só é aplicada depois de conferir o SHA-256 contra o `SHA256SUMS.txt` do release e, quando o fx for assinado, a assinatura do instalador.
