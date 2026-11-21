# Arquitetura

Regra central: **a interface só desenha.** Tudo que pode demorar roda em workers ou em processos separados, e se comunica com a UI por mensagens.

```
┌──────────────────────────────────────────────────────────────┐
│ fx-ui  (binário `fx`) — GPUI, thread de UI                   │
│   lista virtualizada · abas · paleta · Quick Look            │
└───────────────▲──────────────────────────────┬───────────────┘
                │ eventos/estado (canal)       │ comandos
┌───────────────┴──────────────────────────────▼───────────────┐
│ fx-core — estado, comandos, ordenação, filtro, histórico     │
│   sem Windows, sem I/O, 100% testável, #![forbid(unsafe)]    │
└──────┬──────────────┬───────────────┬───────────────┬────────┘
       │              │               │               │
  ┌────▼─────┐  ┌─────▼─────┐  ┌──────▼─────┐  ┌──────▼─────┐
  │ fx-fs    │  │ fx-fs     │  │ fx-shell   │  │ fx-ops     │
  │ leitura  │  │ observador│  │ COM (STA): │  │ fila de    │
  │ de pastas│  │ de mudança│  │ menu, mini-│  │ operações, │
  │          │  │           │  │ aturas, D&D│  │ desfazer   │
  └──────────┘  └───────────┘  └─────┬──────┘  └────────────┘
       │ named pipe                  │
  ┌────▼───────────────────┐   ┌─────▼──────────────────────┐
  │ fx-indexd (elevado)    │   │ fx-preview (processo)      │
  │ MFT + USN → índice     │   │ preview handlers isolados  │
  └────────────────────────┘   └────────────────────────────┘
```

## Crates

| Crate | Tipo | Responsabilidade | Pode depender de |
|---|---|---|---|
| `fx-core` | lib | Tipos de domínio (`Entry`, `Folder`, `Selection`), ordenação, filtro local, comandos, histórico de desfazer, estado de abas | só crates puros (sem `windows`) |
| `fx-fs` | lib | Enumeração de pastas em lote, metadados, observador de mudanças | `fx-core`, `windows` |
| `fx-index` | lib | Leitura da MFT e do diário USN, índice em memória, busca difusa | `fx-core`, `windows`, `ntfs-reader` |
| `fx-indexd` | bin | Serviço elevado que mantém o índice e responde buscas por named pipe (Fase 2) | `fx-index` |
| `fx-shell` | lib | Ponte COM: menu de contexto, miniaturas, arrastar e soltar, ícones, registro como padrão | `fx-core`, `windows` |
| `fx-ops` | lib | Motor de operações (copiar, mover, renomear, excluir para Lixeira), fila, conflitos | `fx-core`, `fx-fs`, `windows` |
| `fx-ui` | bin `fx` | Interface GPUI | todos acima |
| `fx-bench` | bin | Gerador de fixtures e benchmarks | todos acima |

`fx-core` **nunca** depende de `windows` nem de GPUI. Isso mantém a lógica testável em qualquer máquina.

## Modelo de threads

- **Thread de UI (GPUI):** só desenha e recebe input. Lê o estado de `fx-core` e envia comandos.
- **Pool de leitura (fx-fs):** enumeração de pastas em paralelo. Entrega resultados em lotes (ex.: a cada 2.000 entradas ou 8 ms, o que vier primeiro) para a UI pintar cedo.
- **Thread do observador:** `ReadDirectoryChangesW` por pasta aberta; manda diffs, nunca a lista inteira.
- **Thread STA do shell (fx-shell):** COM exige apartment single-threaded para o shell. Uma thread dedicada recebe pedidos (menu, miniatura, ícone) por canal e responde de forma assíncrona. Chamadas a extensões de terceiros têm tempo limite.
- **Thread de miniaturas (prioridade baixa):** primeira passada só com o cache do Windows (`WTS_INCACHEONLY`); extração do resto em segundo plano.
- **Motor de operações (fx-ops):** uma thread por operação ativa, com fila e controle de concorrência por unidade de disco.

Comunicação entre threads: canais (`crossbeam-channel` ou o executor do GPUI). Sem locks segurados durante I/O.

## APIs do Windows previstas

| Necessidade | API |
|---|---|
| Enumerar pasta rápido | `FindFirstFileExW` com `FindExInfoBasic` + `FIND_FIRST_EX_LARGE_FETCH`; avaliar `NtQueryDirectoryFile` / `GetFileInformationByHandleEx(FileIdBothDirectoryInfo)` na Fase 0 |
| Observar mudanças | `ReadDirectoryChangesW` (ou `ReadDirectoryChangesExW`) |
| Ler MFT / USN | crate `ntfs-reader` (requer admin); `FSCTL_QUERY_USN_JOURNAL`, `FSCTL_READ_USN_JOURNAL` |
| Miniaturas | `IShellItemImageFactory::GetImage` e `IThumbnailCache::GetThumbnail` |
| Menu de contexto | `IShellFolder::GetUIObjectOf` → `IContextMenu` / `IContextMenu3` |
| Arrastar e soltar | OLE `IDropTarget` / `IDataObject` / `DoDragDrop` |
| Lixeira e operações compatíveis | `IFileOperation` (para Lixeira e desfazer do shell) e `CopyFile2` (cópia com progresso) |
| Abrir arquivo com o app padrão | `ShellExecuteExW` |
| Virar o gerenciador padrão (Fase 3) | `HKCU\Software\Classes\Directory\shell` e `Drive\shell` + interceptar Win+E |

## Desenho da lista (desempenho)

- **Virtualização:** só as linhas visíveis (+ uma margem) são construídas a cada quadro.
- **Dados em colunas (struct of arrays):** nomes num buffer único de `u16`/UTF-8 com offsets, tamanhos e datas em vetores separados. Ordenar = ordenar um vetor de índices.
- **Ordenação natural** ("arquivo2" antes de "arquivo10"), igual ao Explorador (`StrCmpLogicalW` como referência de comportamento).
- **Nada de realocar a lista inteira a cada lote** — anexar e reordenar índices.
