# fx

Gerenciador de arquivos para Windows 10/11: abre qualquer pasta na hora, acha qualquer arquivo do disco em milissegundos e pode substituir o Explorador. Open source, escrito em Rust.

> Status: **Fase 2 — busca, prévias e beta pública.** A Fase 1 (navegação) está pronta, mas ainda é pré-beta: use com cuidado.

## Instalar

Cada versão fica em [Releases](../../releases) de duas formas:

- **Instalador** (`fx-X.Y.Z-setup-x64.exe`): instala só para o seu usuário, sem pedir administrador, e cria o atalho no Menu Iniciar. Desinstala por Configurações → Aplicativos, sem deixar nada (pergunta se você quer apagar também as configurações).
- **Portátil** (`fx-X.Y.Z-windows-x64.zip`): descompacte e rode `fx.exe`.

Até a 1.0 as versões são pré-lançamento. Enquanto a assinatura digital não estiver ativa, o Windows pode avisar "editor desconhecido": clique em "Mais informações" → "Executar assim mesmo". O que mudou em cada versão está no [`CHANGELOG.md`](CHANGELOG.md). A versão e o commit de um `fx.exe` aparecem nas Propriedades do arquivo e no comando "Sobre o fx" da paleta.

## Usar

**Ctrl+K abre a paleta**: todos os comandos, com o atalho ao lado, mais as pastas recentes e o Acesso rápido. Os principais atalhos:

| Atalho | Ação | Atalho | Ação |
|---|---|---|---|
| Ctrl+K | Paleta de comandos | Ctrl+F | Buscar (pasta ou disco inteiro) |
| Espaço | Pré-visualizar (Quick Look) | F2 | Renomear (vários itens: em lote) |
| Ctrl+C / X / V | Copiar, recortar, colar | Ctrl+Shift+C | Copiar caminho |
| Delete | Lixeira | Shift+Delete | Excluir permanentemente |
| Ctrl+Z / Ctrl+Y | Desfazer, refazer | Ctrl+Shift+H | Histórico de operações |
| Ctrl+Shift+N | Nova pasta | Alt+Enter | Propriedades |
| Ctrl+T / Ctrl+W | Nova aba, fechar aba | Ctrl+Tab | Próxima aba |
| Alt+← / Alt+→ / Alt+↑ | Voltar, avançar, subir | Ctrl+L | Editar o caminho |
| Ctrl+H | Itens ocultos | Ctrl+B | Barra lateral |
| Ctrl+1 / 2 / 3 | Densidade | Ctrl+Shift+2 / 6 | Grade / lista |
| Ctrl+` | Terminal na pasta | Shift+F10 | Menu de contexto |

Digitar na lista filtra a pasta na hora. Na busca (Ctrl+F), Tab alterna entre a pasta atual e o disco inteiro, e dá para filtrar digitando junto com o nome: `tipo:imagem`, `ext:pdf,docx`, `tamanho:>100mb`, `data:semana` (ou `>2026-01-01`).

**Busca no disco inteiro:** pela paleta, "Ativar a busca no disco inteiro" instala o serviço opcional `fx-indexd` (pede administrador uma vez). Ele indexa os discos NTFS e se mantém atualizado; a busca responde em milissegundos e a coluna Tamanho das pastas vem pronta. "Desativar…" remove tudo.

## Relatar problemas

Abra uma [issue](../../issues/new/choose) com o que você fez, o que esperava e o que aconteceu, e a versão (paleta → "Sobre o fx"). Problemas de perda de dados têm prioridade máxima: descreva os passos exatos.

## Benchmarks

Fase 0, num Ryzen 7 5700X com NVMe e monitor de 239 Hz, cache quente, build release. Detalhes, método e pendências em [`bench/results/gate-0.md`](bench/results/gate-0.md).

| Medida | Meta | fx | Explorador | Files |
|---|---|---|---|---|
| Pasta com 100 mil arquivos, primeira pintura (app já aberto) | ≤ 100 ms | **24 ms** | 264 ms | — |
| Pasta com 100 mil arquivos, janela nova (processo novo) | ≤ 100 ms | 386–406 ms ¹ | 1.048 ms | — |
| Lista completa e ordenada, 100 mil (app aberto) | ≤ 300 ms | **86–117 ms** | — | — |
| Tecla → quadro (digitar para filtrar), p95 | ≤ 16 ms | **3,4 ms** | — | — |
| Busca por nome no disco inteiro, por tecla, p95 | ≤ 50 ms | **4,1 ms** ² | — | — |
| Índice do disco (MFT) | ≤ 10 s | **7,6 s** ² | — | — |
| Memória parado | ≤ 80 MB | 72 MB | — | — |

¹ Limitado pela inicialização do driver da GPU (~200 ms); ver `docs/DECISIONS.md` D-013. ² Disco C: real, com 763 mil arquivos (a meta é para ~2 milhões). O Files não estava instalado na máquina de teste.

## Desenvolver

Requisitos: Windows 10/11 com WSL, e no WSL: [rustup](https://rustup.rs) (o toolchain é instalado sozinho a partir de `rust-toolchain.toml`), [`cargo-xwin`](https://github.com/rust-cross/cargo-xwin), `clang`, `lld` e `llvm-rc`. O build é cruzado para Windows e o app roda no Windows pela interoperabilidade do WSL (ver `docs/DECISIONS.md`, D-010).

```bash
cargo xwin build
cargo xwin run -p fx-ui --release
```

Documentação para quem contribui (pessoa ou agente): comece por [`CLAUDE.md`](CLAUDE.md) e [`docs/`](docs/).

## Licença

Dupla licença, à sua escolha: [MIT](LICENSE-MIT) ou [Apache 2.0](LICENSE-APACHE). O código copiado em `third_party/gpui` (GPUI, da Zed Industries) é Apache 2.0 e traz um patch próprio, descrito em `third_party/gpui/FX-PATCH.md`.
