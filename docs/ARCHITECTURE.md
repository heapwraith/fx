# Arquitetura

Regra central: **a interface só desenha.** Tudo que pode demorar roda em threads de trabalho ou em processos separados, e conversa com a interface por mensagens. Nada de disco, rede, COM do shell ou espera de lock na thread de UI.

```
┌───────────────────────────────────────────────────────────────────┐
│ fx  (fx-ui) — GPUI, thread de UI                                  │
│   abas · lista/grade/colunas · paleta · busca · Quick Look        │
│   painel duplo · terminal · atualizador · Sobre / Como usar       │
└──────▲─────────────────────────────────────────────────┬──────────┘
       │ estado e eventos (canais)                       │ comandos
┌──────┴─────────────────────────────────────────────────▼──────────┐
│ fx-core — lógica pura: lista, ordenação, filtro, busca, desfazer, │
│ abas, etiquetas, tela VT, pacote do instalador, versões, idioma   │
│ sem Windows, sem I/O de sistema, #![forbid(unsafe_code)]          │
└──────┬──────────────┬──────────────────┬──────────────────┬───────┘
  ┌────▼────┐   ┌─────▼─────┐   ┌────────▼────────┐   ┌─────▼──────┐
  │ fx-fs   │   │ fx-index  │   │ fx-shell        │   │ fx-ops     │
  │ pastas, │   │ MFT, USN, │   │ COM (STA): menu,│   │ fila de    │
  │ observa-│   │ índice,   │   │ ícones, miniatu-│   │ operações, │
  │ dor,    │   │ busca,    │   │ ras, arrastar,  │   │ conflitos, │
  │ busca em│   │ protocolo │   │ ConPTY, rede,   │   │ diário,    │
  │ pasta   │   │ do pipe   │   │ registro, insta-│   │ Lixeira    │
  └─────────┘   └─────┬─────┘   │ lação           │   └────────────┘
                      │ pipe    └────────┬────────┘
            ┌─────────▼────────┐ ┌───────▼────────┐ ┌────────────────┐
            │ fx-indexd        │ │ fx-preview     │ │ fx-setup       │
            │ serviço SYSTEM:  │ │ processo: pre- │ │ instalador,    │
            │ índice de todos  │ │ view handlers  │ │ desinstalador, │
            │ os volumes NTFS  │ │ do Windows     │ │ aplica updates │
            └──────────────────┘ └────────────────┘ └────────────────┘
```

## Crates

| Crate | Tipo | Responsabilidade | Depende de |
|---|---|---|---|
| `fx-core` | lib | Tipos de domínio, lista em colunas (struct of arrays), ordenação natural, filtro, filtros de busca (`tipo:`, `ext:`…), desfazer, abas, colunas Miller, etiquetas, tela VT, Markdown, realce de sintaxe, pacote do instalador, versões e JSON do atualizador, relatório de falhas, idioma (`tr!`/`trf!`) | nada do projeto |
| `fx-fs` | lib | Enumeração de pastas em lote, observador de mudanças, busca recursiva em pasta, tamanho de pastas, marcar arquivos de nuvem, e as chamadas ao sistema do motor de operações (`CopyFile2` com progresso, renomear sem substituir, Lixeira com `IFileOperation`) | `fx-core` |
| `fx-index` | lib | Leitura da MFT e do diário USN, índice em memória, busca difusa, protocolo do pipe (`FXQ3`) | `fx-core` |
| `fx-indexd` | bin | Serviço do Windows (LocalSystem) que mantém o índice e responde por pipe com ACL do usuário | `fx-index` |
| `fx-shell` | lib | Tudo do shell e do sistema: menu de contexto, ícones e miniaturas, arrastar e soltar, área de transferência, Propriedades, abrir com, elevação, UI Automation, instância única, ConPTY, alto contraste, gerenciador padrão, rede/hash/assinatura do atualizador, compressão, atalhos e registro do instalador | `fx-core` |
| `fx-ops` | lib | Motor de operações (copiar, mover, renomear, Lixeira, sobre as primitivas do `fx-fs`), fila, conflitos, diário de temporários, desfazer | `fx-core`, `fx-fs` |
| `fx-theme` | lib | Tokens visuais (Fluent 2): cores dos temas e do alto contraste, espaçamento, tipografia, densidades, ícones | `fx-shell`, `gpui` |
| `fx-ui` | bin `fx` | A interface GPUI | todos os acima |
| `fx-preview` | bin | Hospeda um preview handler do Windows por vez, isolado: um handler de terceiros que trava ou cai só encerra esse processo | `fx-shell` |
| `fx-setup` | lib + bin | Instalador, desinstalador e ajudante de atualização, em GPUI com os tokens do `fx-theme` | `fx-core`, `fx-shell`, `fx-theme` |
| `fx-bench` | bin | Fixtures e benchmarks (P1–P9) | `fx-core`, `fx-fs`, `fx-index` |

`unsafe` só existe em `fx-shell`, `fx-fs` e `fx-index`, em funções pequenas com comentário `// SAFETY:`. `fx-core`, `fx-ops`, `fx-theme`, `fx-ui` e `fx-setup` proíbem `unsafe` (`#![forbid(unsafe_code)]`).

**Esperas na interface (D-066):** o GPUI roda as tarefas de fundo no pool de threads do Windows, cujos itens devem ser curtos. Nada espera canal com `recv()` dentro delas: quem espera é uma thread própria (`fx-ui/src/chan.rs`) ou um canal do `futures`, aguardado sem bloquear. Rede e downloads (atualização) também rodam em thread própria.

**Acessibilidade (D-008, D-066):** cada janela tem uma árvore do AccessKit (`fx-ui/src/a11y.rs`, adaptador UIA no `fx-shell`): janela, caminho, lista (só as linhas visíveis, com "x de N") e, por cima, o diálogo ou painel com o teclado, em três camadas: painéis da aba, diálogos da janela e a confirmação. Quem desenha uma sobreposição declara o que o leitor de tela deve ler; a árvore só é enviada quando muda e quando há um leitor de tela ouvindo.

**Registro (D-066):** avisos e erros do fx e do GPUI vão para `%LOCALAPPDATA%\fx\logs\fx.log` (máx. 1 MB, mais `fx.old.log`), escritos por uma thread própria; `FX_LOG=info|debug|trace` registra mais.

## Plataformas (D-012)

Só Windows é suportado e medido por enquanto, mas o código do Windows fica isolado:

- `fx-core` não conhece o Windows; `fx-ui` e `fx-setup` falam com o sistema só pelas interfaces de `fx-fs`, `fx-shell`, `fx-index` e `fx-ops`.
- Cada um desses crates expõe tipos neutros na raiz e guarda a implementação em `src/windows/` (`#[cfg(windows)]`). Tipos do `windows-rs` nunca aparecem na API pública.
- Não há stubs para outros sistemas. Escrever a segunda implementação é o trabalho do porte; a interface (GPUI) já roda em macOS e Linux.

## Processos

| Processo | Quando roda | Fala com o fx por |
|---|---|---|
| `fx.exe` | sempre; uma instância por usuário (as outras encaminham a pasta por pipe e saem) | — |
| `fx-indexd.exe` | se o usuário ativar a busca no disco inteiro (serviço, cópia em `%ProgramFiles%\fx`) | pipe nomeado, protocolo `FXQ3` |
| `fx-preview.exe` | ao pré-visualizar um tipo que tem preview handler do Windows | entrada e saída padrão; janela própria sobre o painel |
| `fx-setup.exe` | ao instalar, desinstalar ou aplicar uma atualização (espera o fx fechar) | linha de comando |

## Threads do fx

- **UI (GPUI):** desenha e recebe entrada; lê o estado e envia comandos.
- **Leitura de pastas (fx-fs):** entrega em lotes (a cada 2.000 entradas ou 8 ms) para pintar cedo.
- **Observador:** `ReadDirectoryChangesW` na pasta aberta; manda diferenças, nunca a lista inteira.
- **STA do shell (fx-shell):** menus, ícones, miniaturas e Propriedades numa thread COM dedicada, com tempo limite para extensões de terceiros.
- **Operações (fx-ops):** uma thread por operação, com fila.
- **Outras, sob demanda:** busca, tamanhos de pasta, ConPTY do terminal, gravação da sessão (SQLite) e das etiquetas, atualizador.

## APIs do Windows usadas

| Necessidade | API |
|---|---|
| Ler pastas | `FindFirstFileExW` (`FindExInfoBasic`, `FIND_FIRST_EX_LARGE_FETCH`) |
| Observar mudanças | `ReadDirectoryChangesW` |
| Índice do disco | MFT e diário USN (`FSCTL_QUERY_USN_JOURNAL`, `FSCTL_READ_USN_JOURNAL`), em serviço |
| Ícones e miniaturas | `SHGetFileInfo`, `IShellItemImageFactory` |
| Menu de contexto | `IContextMenu` / `IContextMenu3` |
| Arrastar e soltar | OLE `IDropTarget`, `IDataObject`, `DoDragDrop` |
| Copiar | `CopyFile2` com progresso; Lixeira por `IFileOperation` |
| Prévias | `IPreviewHandler` no `fx-preview` |
| Terminal | ConPTY (`CreatePseudoConsole`) |
| Gerenciador padrão | `HKCU\Software\Classes` (Folder, Directory, Drive, Win+E), com backup e reverter |
| Atualizador | WinHTTP, BCrypt (SHA-256), WinVerifyTrust — carregadas só quando usadas (*delay-load*) |
| Instalador | Compression API (LZMS), `IShellLinkW`, `HKCU\…\Uninstall\fx` |

## Desenho da lista

- **Virtualização:** só as linhas visíveis (mais uma margem) são construídas a cada quadro.
- **Dados em colunas:** nomes num buffer único com deslocamentos, tamanhos e datas em vetores separados; ordenar é ordenar um vetor de índices.
- **Ordenação natural** ("arquivo2" antes de "arquivo10"), como o Explorador.
- **Lotes anexados**, nunca a lista inteira realocada; reordenação em segundo plano.
