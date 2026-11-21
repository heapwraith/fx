# Fases 1 a 4 — visão geral

Estas fases ficam propositalmente em nível de entregas. **Ao passar o portão de uma fase, o agente detalha a próxima** num arquivo `PHASE-N.md` no mesmo formato da Fase 0 (tarefas pequenas, ordem, aceite verificável) e pede aprovação do usuário antes de começar.

## Fase 1 — MVP de navegação (≈ 10 semanas)

Entregas:
- Janela com abas (Ctrl+T, Ctrl+W, Ctrl+Tab, arrastar aba), histórico voltar/avançar por aba.
- Barra de caminho em migalhas que vira campo de texto ao clicar, com autocompletar.
- Lateral: Acesso rápido (fixar/desafixar), unidades com espaço livre, pastas do usuário.
- Vistas: lista (detalhes) e grade (ícones/miniaturas). Densidade compacta/padrão/confortável.
- Miniaturas e ícones via `fx-shell` (cache primeiro, extração em segundo plano).
- Menu de contexto do shell completo (inclui extensões de terceiros), com tempo limite.
- Observador de mudanças: a lista atualiza sozinha quando arquivos mudam.
- `fx-ops` v1: copiar, recortar, colar, renomear (F2), nova pasta, excluir para a Lixeira; fila visível com progresso, pausa e cancelamento; diálogo único de conflitos; Ctrl+Z / Ctrl+Y.
- Arrastar e soltar entre o fx, o Explorador e outros apps, nos dois sentidos.
- Abrir arquivo com o app padrão; "Abrir com"; "Abrir no Terminal".
- Tema claro/escuro seguindo o Windows; fundo Mica.
- Persistência: abas e janelas restauradas ao reabrir (TOML + SQLite).

**Portão:** o usuário usa o fx como gerenciador principal por 2 semanas sem voltar ao Explorador; metas P1–P4 mantidas; nenhum caso de perda de dados.

## Fase 2 — Busca, Quick Look e paleta (≈ 8 semanas)

Entregas:
- `fx-indexd`: serviço elevado opcional (instalado com consentimento), mantém o índice de todas as unidades NTFS e responde por named pipe; o app funciona sem ele (busca cai para enumeração + Windows Search).
- Ctrl+F: busca no disco inteiro com filtros (tipo, tamanho, data, pasta), resultados enquanto digita.
- Quick Look (Espaço): imagem, vídeo, áudio, PDF, texto/código com cores (tree-sitter), Markdown renderizado, conteúdo de ZIP; processo `fx-preview` isolado para preview handlers de terceiros.
- Paleta de comandos (Ctrl+K) com busca difusa sobre comandos, pastas recentes e configurações.
- Renomear em lote no lugar, com prévia ao vivo, numeração e regex.
- Tamanho de pastas a partir do índice.
- Testes de falha do motor de operações (ver SAFETY.md §3) completos.
- Beta pública no GitHub (instalador + portátil), assinatura via SignPath Foundation.

**Portão:** 100 testadores; zero perda de dados; metas P1–P9 mantidas; taxa de falhas conhecida.

## Fase 3 — Virar o padrão (≈ 8 semanas)

Entregas:
- Opção "Definir como gerenciador de arquivos padrão" (HKCU, sem admin) e interceptação de Win+E, com botão de reverter que restaura tudo. **Só implementar com aprovação explícita do usuário (SAFETY.md §1).**
- Painel duplo (F3), vista em colunas (Miller), tags, terminal embutido opcional.
- Build ARM64 (`aarch64-pc-windows-msvc`).
- Interface em português e inglês; Narrador e alto contraste completos; DPI por monitor.
- Integração com pastas de nuvem (OneDrive etc.) com ícones de status.
- Relatório de falhas opcional e anônimo (opt-in).

**Portão:** ≥ 99,5% das sessões da beta sem falha; substituição do padrão funcionando nos pontos documentados.

## Fase 4 — Versão 1.0 (≈ 4 semanas)

Entregas:
- Pacote MSIX para a Microsoft Store (assinatura gratuita pela Store), manifesto winget, instalador e portátil no GitHub.
- Atualizador próprio (pacotes delta, verificação de assinatura) para as versões do GitHub.
- Site e documentação de uso; vídeo de 30 s lado a lado com o Explorador.
- Polimento final: animações ≤ 150 ms, estados vazios, mensagens de erro claras.

**Portão:** lançamento.
