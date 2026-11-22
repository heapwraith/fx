# Fase 4 — Atualizador e polimento

> **Iniciada em 2026-11-22.** O portão da Fase 3 está em `bench/results/gate-3.md`. A pedido do usuário, a Fase 4 tem só duas entregas, o atualizador próprio e o polimento final; tudo o que ficou pendente das fases anteriores (testes manuais, MSIX/Store, winget, site, terminal completo etc.) foi reunido na Fase 5 (`PHASE-5.md`), separada para o fim. Ao terminar, o fx é instalado no PC do usuário pelo instalador, para uso diário.

**Objetivo:** o fx se mantém atualizado sozinho a partir das versões do GitHub, sem nunca instalar algo que não seja o pacote publicado, e cada tela, estado vazio e erro está acabado: transições curtas, textos claros nos dois idiomas.

**Regras desta fase:**
- Sem dependências novas: rede pelo WinHTTP e hash pelo BCrypt do Windows (`windows-rs`, já no projeto).
- Nada é baixado nem instalado sem o usuário pedir; a verificação de versão pode ser desligada.
- P8 (memória parado ≤ 88 MB) está a 1,1 MB do limite: nada desta fase pode aumentá-la.
- Toda tela segue a skill `fx-design`; P4 medido.

---

## T4.1 — Atualizador
- [ ] Concluída em: ____ · Resumo:

**Fazer**
- Uma vez por dia, fora da thread de UI, o fx consulta os releases do GitHub (`api.github.com/repos/heapwraith/fx/releases`, WinHTTP, sem dados do usuário além do IP) e compara a versão (semver, inclusive pré-versões). Se houver uma mais nova, o status avisa e a paleta oferece "Atualizar para X".
- Atualizar baixa o instalador da arquitetura certa (`setup-x64`/`setup-arm64`) para `%TEMP%`, confere o SHA-256 (BCrypt) contra o `SHA256SUMS.txt` publicado no mesmo release (o CI passa a gerá-lo) e, se o fx em uso for assinado, exige o instalador assinado (WinVerifyTrust). Só então roda o instalador em modo silencioso e fecha o fx.
- Opção `verificar_atualizacoes = false` (e comando na paleta) desliga a consulta.
- Pacotes delta: avaliar; o instalador tem ~6 MB, então só entram se o ganho justificar (registrar a decisão).

**Aceite**
- Testes: comparação de versões (0.2.0-beta < 0.2.0 < 0.10.0), leitura do JSON dos releases e escolha do instalador, verificação do hash (arquivo certo passa, alterado falha, ausente no SHA256SUMS falha), nada baixado com a opção desligada. Conferência real contra o GitHub (consulta) sem instalar.

## T4.2 — Polimento final
- [ ] Concluída em: ____ · Resumo:

**Fazer**
- Transições ≤ 150 ms nas sobreposições (paleta, menus, busca, Quick Look, diálogos): entrada curta, nunca atrasando a próxima tecla; respeitar "efeitos de animação" desligados no Windows.
- Estados vazios e erros revisados em todas as telas: uma frase com o que houve e o próximo passo, nos dois idiomas; as mensagens de erro que vinham de `fx-ops`/`fx-shell` só em português passam a ter as duas versões.
- Passada visual com a skill `fx-design` (claro/escuro, três densidades, janela estreita): corrigir o que estiver cortado, desalinhado ou inconsistente.

**Aceite**
- P4 e P8 sem regressão; teste de que nenhuma animação passa de 150 ms e de que com animações desligadas nada anima; varredura de textos (T3.7) estendida a `fx-ops` e `fx-shell`; capturas conferidas.

## T4.3 — Relatório do portão e instalação no PC do usuário
- [ ] Concluída em: ____ · Resumo:

**Fazer**
- `bench/results/gate-4.md` com as metas e os riscos.
- Gerar o instalador (release) e instalar no Windows do usuário, por usuário (sem administrador), conferindo que abre pelo Menu Iniciar.

---

## Portão da Fase 4

1. Atualizador testado (versão, hash, assinatura quando houver) e verificado contra o GitHub.
2. P1–P9 mantidas.
3. fx instalado e em uso no PC do usuário.
