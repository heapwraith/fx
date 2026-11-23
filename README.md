# Pathdash

**Qualquer pasta, numa arrancada.** Pastas abrem na hora e arquivos aparecem enquanto você digita.

O Pathdash é um gerenciador de arquivos para Windows 10 e 11, nativo, open source e escrito em Rust. Ele abre uma pasta com 100 mil arquivos em cerca de 10 ms, busca no disco inteiro enquanto você digita e pode substituir o Explorador como padrão do sistema, com volta garantida.

> **Status:** versão **0.3.0-beta**. As fases 0 a 4 estão concluídas: navegação, busca, prévias, recursos avançados, instalador próprio e atualizador. A Fase 5 reúne o que falta para a 1.0, como testes manuais, assinatura digital e publicação; a Fase 6 traz os diferenciais depois dela (mapa do disco, linha do tempo, buscas vivas e mais). Veja o [roteiro](docs/ROADMAP.md).

## Por que o Pathdash

| | Pathdash | Explorador |
|---|---|---|
| Abrir uma pasta de 100 mil arquivos (programa aberto) | **~10 ms** | ~254 ms |
| Primeira abertura (processo novo) até a lista | **~430 ms** | ~1.050 ms |
| Tecla → tela atualizada (p95) | **≤ 4,4 ms** | — |
| Busca por nome no disco inteiro, por tecla (p95) | **~5 ms** | — |
| Memória parado | ~87 MB | +117–125 MB por janela |

Medições em release num Ryzen 7 5700X com NVMe e monitor de 239 Hz. O método está em [`docs/PERFORMANCE.md`](docs/PERFORMANCE.md) e os números completos, em [`bench/results/gate-4.md`](bench/results/gate-4.md).

## O que ele faz

- **Navegação rápida:** abas, barra de caminho com autocompletar, Acesso rápido e unidades, lista, grade ou colunas, painel duplo (F3), três densidades.
- **Busca:** filtra a pasta enquanto você digita; Ctrl+F busca na pasta e nas subpastas ou no disco inteiro (lendo a MFT do NTFS), com filtros como `tipo:imagem`, `ext:pdf`, `tamanho:>100mb`, `data:semana` e `etiqueta:verde`.
- **Quick Look (Espaço):** imagens, texto e código com cores, Markdown, conteúdo de ZIP e os visualizadores do próprio Windows (PDF, Office, mídia).
- **Operações seguras:** copiar, mover, renomear, renomear em lote e enviar para a Lixeira, com fila, conflitos sem sobrescrever e desfazer (Ctrl+Z) de qualquer operação.
- **Organização:** etiquetas coloridas (Alt+1…7), coluna Tamanho com o total de cada pasta, estado das pastas de nuvem (OneDrive e outras).
- **Para quem usa o teclado:** paleta de comandos (Ctrl+K), terminal embutido (Ctrl+J), todos os atalhos do Explorador.
- **Integração com o Windows:** menu de contexto e arrastar e soltar do sistema, Propriedades, tema claro e escuro com Mica, alto contraste, leitor de tela, português e inglês.
- **Pode virar o padrão:** pastas, unidades e Win+E abrem no Pathdash; "Voltar ao Explorador" desfaz tudo.

## Instalar

Baixe a última versão em [Releases](../../releases):

- **Instalador** (`Pathdash-X.Y.Z-setup-x64.exe`, ou `-arm64.exe`): instala só para o seu usuário, sem pedir administrador, e coloca o Pathdash no Menu Iniciar. Desinstala por Configurações › Aplicativos.
- **Portátil** (`Pathdash-X.Y.Z-windows-x64.zip`): descompacte e rode `pathdash.exe`.

**Atualizações:** o Pathdash procura uma versão nova uma vez por dia, baixa e confere em segundo plano, e aplica quando você fecha o programa. "Atualizar o Pathdash" na paleta reinicia na hora; Configurações › Atualizações desliga.

Enquanto a assinatura digital não está ativa, o Windows pode avisar "editor desconhecido": clique em "Mais informações" e depois em "Executar assim mesmo".

## Primeiros passos

**F1** (ou "Ajuda", no rodapé da barra lateral) mostra como usar o Pathdash, **Ctrl+K** encontra qualquer comando pelo nome e **Ctrl+,** abre as Configurações. Os atalhos principais:

| Atalho | Ação | Atalho | Ação |
|---|---|---|---|
| Ctrl+K | Paleta de comandos | F1 | Como usar o Pathdash |
| Ctrl+F | Buscar (Tab: disco inteiro) | Espaço | Pré-visualizar (Quick Look) |
| Enter | Abrir | Alt+↑ / ← / → | Subir, voltar, avançar |
| F2 | Renomear (vários: em lote) | Ctrl+Shift+N | Nova pasta |
| Ctrl+C / X / V | Copiar, recortar, colar | Ctrl+Shift+C | Copiar caminho |
| Delete | Lixeira | Shift+Delete | Excluir de vez |
| Ctrl+Z / Ctrl+Y | Desfazer, refazer | Ctrl+Shift+H | Histórico de operações |
| Ctrl+T / Ctrl+W | Nova aba, fechar aba | F3 | Painel duplo (F5 copia, F6 move) |
| Ctrl+Shift+3 | Vista em colunas | Ctrl+J | Terminal embutido |
| Alt+1…7 | Etiqueta colorida | Ctrl+H | Itens ocultos |
| Ctrl+L | Digitar um caminho | Alt+Enter | Propriedades |

**Busca no disco inteiro:** "Ativar a busca no disco inteiro", na paleta, instala o serviço opcional `fx-indexd` e pede administrador uma vez. Ele mantém um índice dos discos NTFS, e a busca responde em milissegundos. "Desativar a busca no disco inteiro" remove o serviço.

## Privacidade

O Pathdash não envia nada sobre você. A verificação de atualizações só consulta a lista de versões no GitHub. Se o Pathdash fechar com uma falha, ele grava um relatório local, sem caminhos nem nomes, e oferece abrir uma issue que você revisa antes de enviar. As duas coisas podem ser desligadas em Configurações.

## Relatar problemas

Abra uma [issue](../../issues/new/choose) com o que você fez, o que esperava, o que aconteceu e a versão (paleta › "Sobre o Pathdash"). Problemas de perda de dados têm prioridade máxima: descreva os passos exatos.

## Desenvolver

O Pathdash é desenvolvido no WSL e compilado de forma cruzada para Windows com [`cargo-xwin`](https://github.com/rust-cross/cargo-xwin); os executáveis rodam no Windows pela interoperabilidade do WSL. Requisitos no WSL: [rustup](https://rustup.rs) (o toolchain vem de `rust-toolchain.toml`), `cargo-xwin`, `clang`, `lld` e `llvm-rc`.

```bash
cargo xwin build                       # compila tudo
cargo xwin test --workspace            # testes (rodam no Windows)
cargo xwin run -p fx-ui --release      # abre o Pathdash
cargo xwin run -p fx-bench --release -- all   # benchmarks
```

Para gerar o instalador: `fx-setup --pack <fx-setup.exe> <pasta dos binários> <raiz do repositório> <saída.exe>`. O CI faz isso a cada push.

Para contribuir, pessoa ou agente, comece por [`CLAUDE.md`](CLAUDE.md) e pela pasta [`docs/`](docs/):

| Documento | Conteúdo |
|---|---|
| [`ROADMAP.md`](docs/ROADMAP.md) | Fases, o que cada uma entregou e o que falta |
| [`ARCHITECTURE.md`](docs/ARCHITECTURE.md) | Crates, processos, threads e regras de dependência |
| [`PERFORMANCE.md`](docs/PERFORMANCE.md) | Metas P1–P9 e protocolo de medição |
| [`SAFETY.md`](docs/SAFETY.md) | Segurança de dados e limites de autonomia |
| [`PRODUCT.md`](docs/PRODUCT.md) | Princípios de produto, layout e atalhos |
| [`DECISIONS.md`](docs/DECISIONS.md) | Registro de decisões (D-001 em diante) |
| [`GUIA.md`](docs/GUIA.md) | Guia de uso: atalhos, busca, painéis, etiquetas, terminal |
| [`TESTES-FINAIS.md`](docs/TESTES-FINAIS.md) | Testes manuais pendentes |
| [`SIGNING.md`](docs/SIGNING.md) | Como ativar a assinatura digital |

## Licença

Dupla licença, à sua escolha: [MIT](LICENSE-MIT) ou [Apache 2.0](LICENSE-APACHE).

A interface usa o [GPUI](https://github.com/zed-industries/zed/tree/main/crates/gpui), uma biblioteca open source sob a licença Apache 2.0, incluída em `third_party/gpui` com um patch próprio (`third_party/gpui/FX-PATCH.md`).
