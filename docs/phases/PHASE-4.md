# Fase 4 — Atualizador e polimento

> **Iniciada em 2026-11-22.** O portão da Fase 3 está em `bench/results/gate-3.md`. A pedido do usuário, a Fase 4 tem duas entregas, o atualizador próprio e o polimento final, mais o instalador próprio (multiplataforma na interface, inspirado no Zed); tudo o que ficou pendente das fases anteriores (testes manuais, MSIX/Store, winget, site, terminal completo etc.) foi reunido na Fase 5 (`PHASE-5.md`), separada para o fim. Ao terminar, o fx é instalado no PC do usuário pelo instalador, para uso diário.

**Objetivo:** o fx se mantém atualizado sozinho a partir das versões do GitHub, sem nunca instalar algo que não seja o pacote publicado, e cada tela, estado vazio e erro está acabado: transições curtas, textos claros nos dois idiomas.

**Regras desta fase:**
- Sem dependências novas: rede pelo WinHTTP e hash pelo BCrypt do Windows (`windows-rs`, já no projeto).
- Nada é baixado nem instalado sem o usuário pedir; a verificação de versão pode ser desligada.
- P8 (memória parado ≤ 88 MB) está a 1,1 MB do limite: nada desta fase pode aumentá-la.
- Toda tela segue a skill `fx-design`; P4 medido.

---

## T4.1 — Atualizador
- [x] Concluída em: 2026-11-22 · Resumo: consulta diária aos releases do GitHub (WinHTTP, thread de fundo, desligável), comparação semver e escolha do instalador da arquitetura (`fx-core::update`, leitor JSON próprio), aviso no status e "Atualizar o fx" na paleta; o instalador é baixado, conferido pelo SHA-256 (BCrypt) contra o `SHA256SUMS.txt` que o CI passa a publicar e, se o fx for assinado, pela assinatura (WinVerifyTrust); roda silencioso com `/RELAUNCH` e reabre o fx; delta descartado (instalador de 6 MB) (D-056). 9 testes; consulta real ao GitHub conferida na janela (sem instalar: ainda não há release publicado).

**Fazer**
- Uma vez por dia, fora da thread de UI, o fx consulta os releases do GitHub (`api.github.com/repos/heapwraith/fx/releases`, WinHTTP, sem dados do usuário além do IP) e compara a versão (semver, inclusive pré-versões). Se houver uma mais nova, o status avisa e a paleta oferece "Atualizar para X".
- Atualizar baixa o instalador da arquitetura certa (`setup-x64`/`setup-arm64`) para `%TEMP%`, confere o SHA-256 (BCrypt) contra o `SHA256SUMS.txt` publicado no mesmo release (o CI passa a gerá-lo) e, se o fx em uso for assinado, exige o instalador assinado (WinVerifyTrust). Só então roda o instalador em modo silencioso e fecha o fx.
- Opção `verificar_atualizacoes = false` (e comando na paleta) desliga a consulta.
- Pacotes delta: avaliar; o instalador tem ~6 MB, então só entram se o ganho justificar (registrar a decisão).

**Aceite**
- Testes: comparação de versões (0.2.0-beta < 0.2.0 < 0.10.0), leitura do JSON dos releases e escolha do instalador, verificação do hash (arquivo certo passa, alterado falha, ausente no SHA256SUMS falha), nada baixado com a opção desligada. Conferência real contra o GitHub (consulta) sem instalar.

## T4.2 — Polimento final
- [x] Concluída em: 2026-11-22 · Resumo: as 7 sobreposições (paleta, menu Exibir, busca, Quick Look, renomear em lote, histórico, conflito) entram em 120 ms só na opacidade (`motion.rs`, máx. 150 ms, nada anima com "Efeitos de animação" desligado); busca e paleta sem resultado dizem o que houve e o próximo passo; 27 mensagens de erro de `fx-ops`/`fx-shell` e as do `fx-preview` em pares pt/en; relatório de falhas com rótulos neutros; varredura de textos estendida a `fx-core`, `fx-ops`, `fx-shell`, `fx-fs` (D-057). P4 p95 ≤ 3,45 ms, Quick Look p95 50 ms, P8 86,0–87,6 MB (sem regressão); conferido em claro e escuro.

**Fazer**
- Transições ≤ 150 ms nas sobreposições (paleta, menus, busca, Quick Look, diálogos): entrada curta, nunca atrasando a próxima tecla; respeitar "efeitos de animação" desligados no Windows.
- Estados vazios e erros revisados em todas as telas: uma frase com o que houve e o próximo passo, nos dois idiomas; as mensagens de erro que vinham de `fx-ops`/`fx-shell` só em português passam a ter as duas versões.
- Passada visual com a skill `fx-design` (claro/escuro, três densidades, janela estreita): corrigir o que estiver cortado, desalinhado ou inconsistente.

**Aceite**
- P4 e P8 sem regressão; teste de que nenhuma animação passa de 150 ms e de que com animações desligadas nada anima; varredura de textos (T3.7) estendida a `fx-ops` e `fx-shell`; capturas conferidas.

## T4.3 — Instalador próprio (fx-setup)
- [ ] Concluída em: ____ · Resumo:

**Fazer**
- Novo binário `fx-setup` em Rust + GPUI, com os tokens visuais do fx (Fluent, claro/escuro, português/inglês): um executável só, com o fx comprimido dentro (Compression API do Windows, LZMS; sem crates novas). Substitui o Inno Setup.
- Telas: boas-vindas (logo, versão, "Instalar"; "Opções": atalho na área de trabalho, abrir ao terminar), progresso animado, pronto ("Abrir o fx") e erro ("Tentar de novo"). Atualizar de uma versão instalada diz "Atualizar de X para Y".
- Instala por usuário em `%LOCALAPPDATA%\Programs\fx` (sem administrador): extrai para uma pasta temporária ao lado, confere o SHA-256 de cada arquivo e troca os arquivos de uma vez; se algo falhar, volta ao que havia. Atalho no Menu Iniciar (e na área de trabalho, se pedido); registro em Configurações › Aplicativos (`HKCU\…\Uninstall\fx`), com o próprio `fx-setup` como desinstalador. Uma instalação antiga feita pelo Inno no mesmo lugar é adotada (a chave antiga sai).
- Desinstalar: tela de confirmação ("Apagar também configurações e histórico"), remove arquivos, atalhos e registro.
- A interface do instalador é neutra; as ações de instalação ficam em `fx-shell` atrás de tipos neutros (D-012), só Windows por enquanto.
- O CI gera `fx-X-setup-<arq>.exe` com `fx-setup --pack`.

**Aceite**
- Testes: pacote (empacotar, ler, arquivo corrompido falha), instalação e desinstalação numa pasta temporária e numa chave de registro temporária (`HKCU\Software\fx-test-*`), troca com volta em caso de falha, arquivo em uso (o fx aberto é renomeado, não apagado). Telas conferidas na janela nos dois temas e idiomas.

## T4.4 — Atualização ao reiniciar (como o Zed)
- [ ] Concluída em: ____ · Resumo:

**Fazer**
- A versão nova é baixada e conferida em segundo plano (T4.1) e fica pronta; o status diz "Atualização pronta · reinicie o fx". Ao fechar o fx (ou pelo comando "Reiniciar para atualizar"), o `fx-setup --apply` troca os arquivos sem tela e reabre o fx se foi um reinício. Sai o modo "instalador silencioso que fecha o fx".

**Aceite**
- Teste de ponta a ponta numa pasta temporária: versão "nova" preparada, aplicada ao fechar, versão anterior restaurada se a troca falhar.

## T4.5 — Relatório do portão e instalação no PC do usuário
- [ ] Concluída em: ____ · Resumo:

**Fazer**
- `bench/results/gate-4.md` com as metas e os riscos.
- Instalar o fx no Windows do usuário pelo `fx-setup`, conferindo que abre pelo Menu Iniciar.

---

## Portão da Fase 4

1. Atualizador testado (versão, hash, assinatura quando houver) e verificado contra o GitHub.
2. P1–P9 mantidas.
3. fx instalado e em uso no PC do usuário.
