# O que falta para a 1.0

Situação em 2026-11-24, depois dos portões 5 e 6 (`bench/results/gate-5.md`, `gate-6.md`). As fases 0 a 4 passaram no portão. A Fase 6 tem as 16 tarefas feitas. A Fase 5 tem T5.4 e T5.5 concluídas; T5.1, T5.2, T5.3, T5.6 e T5.7 estão parciais.

## 1. Só o usuário pode fazer

| Item | O que fazer | Onde |
|---|---|---|
| Assinatura digital | Inscrever o projeto na SignPath Foundation. Sem assinatura, o SmartScreen avisa "editor desconhecido", e o portão 5 pede pacote assinado. | `docs/SIGNING.md`, T5.2 |
| Microsoft Store (MSIX) | Criar a conta de desenvolvedor da Store. | T5.2 |
| winget | Autorizar o envio ao `microsoft/winget-pkgs`, que é uma publicação externa. O manifesto já é gerado por `tools/winget-manifest.py`. | T5.2 |
| Serviço do índice no PC do usuário | Configurações › Sistema › **Atualizar serviço**, que pede administrador. Leva para o serviço instalado a correção dos caminhos `\\.\C:` da linha do tempo. | T5.6 |
| Conferir a Fase 6 | Usar o mapa do disco (Ctrl+Shift+M), a linha do tempo (Ctrl+Shift+L) e as buscas salvas, e dizer se estão como deveriam. O portão 6 pede essa conferência. | `gate-6.md` |
| Vídeo de 30 s | Gravar o Pathdash lado a lado com o Explorador. | T5.3 |
| Passar os portões 5 e 6 | A decisão é do usuário. | `gate-5.md`, `gate-6.md` |

## 2. Testes que pedem outro equipamento ou conta

Os outros itens de `docs/TESTES-FINAIS.md` estão feitos, ou cobertos pelo E2E (seção 4).

| Teste | Por que não deu aqui |
|---|---|
| T2.9 visualizadores de Office e mídia no Quick Look | o PC de desenvolvimento não tem Office nem visualizador de mídia registrado (só o de PDF, já testado) |
| T3.9 pastas do OneDrive | precisa de uma conta do OneDrive conectada |
| DPI por monitor (escala de tela) | trocar a escala exige sair e entrar de novo na sessão do Windows |
| T2.11 SmartScreen sem aviso | depende da assinatura (seção 1) |

## 3. Metas que reprovam no PC de referência

| Meta | Hoje | Limite | Próximo passo |
|---|---|---|---|
| P7 rolagem durante cópia de 10 mil arquivos | 0,13–0,28% de quadros perdidos (em 21/11: 0,026%) | ≤ 0,1% | medir com o serviço do índice parado e o PC ocioso; se continuar, perfilar os quadros perdidos com `FX_TRACE` |
| Menu de contexto do shell | p95 de 260–400 ms desde 21/11 (mediana ~60 ms) | ≤ 150 ms | o tempo vem das extensões de terceiros na primeira chamada: pré-carregar em segundo plano na abertura, ou rever a meta |
| P1 e P2 | p95 oscilando logo acima de 450 ms e de 80 ms com o PC carregado (o executável antigo dá o mesmo) | ≤ 450 / ≤ 80 ms | medir com o PC ocioso antes de mexer |
| P6 em NVMe | 1,35 milhão de arquivos em 12,5 s no runner, preso ao disco (112 MB/s); o código gasta 0,54 s | ≤ 10 s para 2 milhões | medir no PC de referência: `fx-bench index --volume C:`, como administrador |

## 4. Como testar o que falta

- **E2E** (`.github/workflows/e2e.yml`): roda depois de cada CI verde no `main`, ou à mão em Actions › E2E. Usa máquinas Windows limpas do GitHub, como administrador sem UAC, e uma Windows on ARM. Cobre:
  - instalação limpa;
  - atualização por cima do Inno e da 0.3;
  - serviço do índice, com P6 e P5;
  - ARM64.

  Resultados no resumo de cada job; capturas nos artefatos `e2e-shots-*`.
- **Benchmarks no PC:** `fx-bench all`, e `fx-bench scroll | type | nav | path | tabs | menu | features | terminal | copy` rodados à mão, porque mandam teclas e cliques. Rodar com o PC livre.
- **Capturas do site:** a pasta de demonstração usada nas imagens de `site/img` foi apagada. Para refazer, crie uma pasta neutra (sem o nome do usuário no caminho, por exemplo em `C:\Users\Public`), abra o Pathdash nela com `FX_NO_STATE=1` e a barra lateral escondida (Ctrl+B).

## 5. Melhorias propostas (não bloqueiam a 1.0)

1. **Trocar "fx" por "Pathdash" por dentro** (pastas `%APPDATA%\fx` e `Programs\fx`, `fx-indexd.exe`, `fx-preview.exe`, `fx-setup.exe`, nome interno do serviço). Exige migração em duas versões (D-074):
   - a primeira move os dados e reinstala o serviço com o nome novo;
   - só depois o repositório pode mudar de nome, porque o atualizador das versões instaladas consulta `heapwraith/fx`.
2. **Linha do tempo sem ruído:** feito em 2026-11-25 — as mudanças que programas fazem nos próprios arquivos (`AppData`, `ProgramData`, pastas `logs`/`cache`/`temp`, `*-journal`, `-wal`, `-shm`, `.log`, `.tmp`, `~$` do Office) ficam ocultas por padrão; o cabeçalho diz quantas, e A mostra todas (`fx-core/src/timeline_noise.rs`). Conferido na janela: 48 de 500 mudanças da última hora eram de programas.
3. **Compilação ARM64 local no WSL** (T5.6): o `psm` não monta com `clang-cl` fora do Windows (D-071). O CI compila e o E2E testa o ARM64.
4. **Site:** prova social (estrelas e opiniões) quando houver, e um vídeo curto no lugar da captura do topo.
