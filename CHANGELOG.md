# Changelog

Todas as mudanças relevantes do fx. Versões seguem o [versionamento semântico](https://semver.org/lang/pt-BR/): enquanto for 0.x, cada fase do projeto sobe o número do meio (Fase 2 → 0.2). O pipeline publica uma versão quando recebe a tag `vX.Y.Z` igual à versão do `Cargo.toml`; as notas do release saem da seção da versão aqui.

## [Não lançado]

### Adicionado
- Ícone do fx e versão nas Propriedades do executável (T2.1).
- Pipeline no GitHub: verificação a cada push e `.exe` portátil a cada versão (T2.2).
- Comando "Sobre o fx (versão)" na paleta, com o commit do build.
- Migalhas que não cabem viram um botão "…" com o menu das pastas escondidas.
- `fx-bench memory` mede a memória parada (P8).

- Testes de falha do motor de operações (disco cheio, arquivo em uso, processo morto, > 4 GB).

### Corrigido
- Uma cópia interrompida porque o fx foi encerrado à força deixava um arquivo temporário no destino; agora ele é apagado na próxima abertura.
- O fx podia fechar sozinho na abertura em certas larguras de janela (texto truncado com reticências).

## [0.1.0] — 2026-11-21

Fase 1 — MVP de navegação. Primeira versão com o pipeline; ainda é pré-beta.

### Adicionado
- Abas com histórico, barra de caminho em migalhas com autocompletar, lateral com Acesso rápido e unidades.
- Lista e grade com ícones e miniaturas do Windows, três densidades, tema claro e escuro, fundo Mica.
- Copiar, recortar, colar, renomear, nova pasta, Lixeira, desfazer e refazer, com fila, pausa e conflitos.
- Arrastar e soltar, menu de contexto completo do Windows, abrir com, abrir no Terminal.
- Ordenar por coluna, coluna Tipo, larguras ajustáveis; itens ocultos (Ctrl+H); Propriedades (Alt+Enter).
- Quick Look (Espaço), busca no disco (Ctrl+F) e paleta de comandos (Ctrl+K).
- Abas, janelas e configurações restauradas ao reabrir; leitor de tela (UI Automation).

### Desempenho (release, Ryzen 7 5700X)
- Pasta com 100 mil arquivos com o fx aberto: 10 ms (Explorador: 254 ms).
- Tecla → quadro: p95 entre 3 e 5 ms em todas as telas medidas.
