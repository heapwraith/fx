# Fase 6 — Superioridade

> Registrada em 2026-11-22, a pedido do usuário, a partir da análise de funcionalidades novas: o que torna o fx **superior** aos concorrentes, não só igual. **Ainda não iniciada:** vem depois da 1.0 (Fase 5), quando o usuário pedir. A ordem abaixo é a recomendada; o usuário pode mudar.

**Objetivo:** usar as vantagens que só o fx reúne — o índice da MFT do disco inteiro em milissegundos, o diário USN (tudo o que muda no disco, em tempo real), o motor de operações seguro com desfazer e a interface nativa — para oferecer o que nenhum gerenciador de arquivos tem junto.

**Regras desta fase:**
- Todas as metas de `docs/PERFORMANCE.md` (D-064) continuam valendo; cada tarefa mede P4 e P8 e não pode passar dos limites.
- Status sempre real: nada simulado.
- Itens com risco de memória (busca no conteúdo, painel de detalhes, busca por significado) só entram se a P8 seguir dentro do limite.
- Toda tela segue a skill `fx-design` e é conferida na janela nos dois idiomas e temas.

---

## Grupo A — Diferenciais (ninguém tem; a base já existe no fx)

### T6.1 — Mapa do disco instantâneo
- [ ] Concluída em: ____ · Resumo:

Treemap de qualquer pasta ou disco mostrando o que ocupa espaço, em milissegundos; clicar navega; Delete libera (com confirmação e desfazer). **Base:** o índice já sabe o tamanho de tudo (totais de pastas em 32 ms por milhão). **Concorrentes:** WizTree/TreeSize são apps separados; o Explorador não tem.

### T6.2 — Linha do tempo do disco
- [ ] Concluída em: ____ · Resumo:

"O que mudou nos últimos 10 minutos, 1 hora, hoje", no disco todo: criados, alterados, apagados; achar "aquele arquivo que baixei agora há pouco". **Base:** o `fx-indexd` já segue o diário USN a cada 250 ms. **Concorrentes:** ninguém expõe o USN ao usuário.

### T6.3 — Buscas vivas
- [ ] Concluída em: ____ · Resumo:

Uma busca vira aba ou pasta virtual que se atualiza sozinha (ex.: `tipo:pdf data:semana`, `tamanho:>1gb`), fixável na barra lateral. **Base:** filtros e serviço prontos; o USN atualiza. **Concorrentes:** o Finder tem pastas inteligentes, mais lentas; Explorador e Files não têm.

### T6.4 — Duplicados em segundos
- [ ] Concluída em: ____ · Resumo:

Acha arquivos repetidos no disco todo: candidatos pelo tamanho (índice), depois SHA-256 só deles; remove com desfazer. **Base:** índice com tamanhos e SHA-256 (BCrypt). **Concorrentes:** ferramentas separadas, lentas porque leem o disco inteiro.

### T6.5 — "Ir para" que aprende
- [ ] Concluída em: ____ · Resumo:

A paleta (Ctrl+K) ordena pastas pela frequência e recência reais de uso (como o zoxide): "proj fx" leva à pasta certa. **Base:** paleta e histórico existem. **Concorrentes:** o Explorador só tem "recentes".

### T6.6 — Desfazer entre sessões
- [ ] Concluída em: ____ · Resumo:

O histórico de operações sobrevive ao fechar o fx ("desfazer a limpeza de ontem"), com as mesmas garantias de nunca sobrescrever. **Base:** histórico e diário do `fx-ops`. **Concorrentes:** Explorador e Files perdem o desfazer ao fechar.

## Grupo B — Paridade que falta, feita do jeito fx

### T6.7 — Arquivos compactados como pastas
- [ ] Concluída em: ____ · Resumo:

Entrar em ZIP, 7z e tar como numa pasta (mesma lista e busca); extrair arrastando; compactar pela paleta. Extração segura: sem caminhos maliciosos, com conflitos e desfazer.

### T6.8 — Busca no conteúdo
- [ ] Concluída em: ____ · Resumo:

Índice de conteúdo opcional e incremental pelo USN (texto e código primeiro), com a busca por tecla de sempre. O Windows Search é lento; o Everything só indexa nomes.

### T6.9 — Painel de detalhes
- [ ] Concluída em: ____ · Resumo:

Lateral direita com metadados reais (dimensões, EXIF, duração, autor), lidos em segundo plano, sem custo de memória parado.

### T6.10 — Colunas de metadados e agrupar por
- [ ] Concluída em: ____ · Resumo:

Dimensões, duração, data da foto como colunas; agrupar por tipo, data ou tamanho, sem travar.

### T6.11 — Comparar pastas e sincronizar
- [ ] Concluída em: ____ · Resumo:

Painel duplo mostrando o que difere entre duas pastas; sincronizar pelo motor seguro, com desfazer.

### T6.12 — Status do Git
- [ ] Concluída em: ____ · Resumo:

Ícone de modificado/novo nos itens de pastas que são repositórios, lido em segundo plano.

### T6.13 — Espaços de trabalho
- [ ] Concluída em: ____ · Resumo:

Conjuntos de abas e painéis por projeto ("Trabalho", "Fotos"), trocados pela paleta.

### T6.14 — Abas entre janelas e mais divisões
- [ ] Concluída em: ____ · Resumo:

Arrastar uma aba para fora vira janela; dividir em mais de dois painéis.

## Grupo C — Tendências de 2026 (opcional e privado)

### T6.15 — Busca por significado
- [ ] Concluída em: ____ · Resumo:

"Fotos da praia de 2024", "contrato com a Ana", usando a IA local do Windows (APIs de IA dos PCs Copilot+), **sem enviar nada**; desligada por padrão e só onde o hardware permite.

### T6.16 — Ações rápidas por tipo
- [ ] Concluída em: ____ · Resumo:

Converter/redimensionar imagens, juntar PDFs, renomear fotos pela data EXIF, direto da seleção, pela paleta.

---

## Análise de 2026-11-22 (D-068)

Revisão das 16 tarefas depois da revisão de código (D-066) e da análise de interface (D-067): como cada uma sai com o que o fx já tem, o que o Windows oferece pronto e o que exigiria dependência nova (que precisa de aprovação, SAFETY.md).

**Regras que valem para toda tarefa desta fase** (aprendidas nas D-066 e D-067):
- Tela nova = teclado completo (Tab, setas, Enter, Esc), anel de foco só ao usar o teclado, alvos de pelo menos 24 px, e a camada de acessibilidade declarada (`a11y::Overlay` ou nós próprios), com "x de N" nas listas.
- Nada de espera bloqueante no pool do GPUI: trabalho longo em thread própria (`chan`), resultados por canal.
- Nomes e caminhos sem conversão com perda (`OsStr`); destinos montados com `fx_core::naming`.
- Conferida nas capturas (`FX_OPEN` na build de depuração) nos dois temas, idiomas e larguras, antes de dizer que está pronta.

| Tarefa | Caminho sem dependência nova | Dependência nova (pedir aprovação) | Risco principal |
|---|---|---|---|
| T6.1 Mapa do disco | Totais de pastas do índice (`fx-indexd`, mensagem nova no protocolo `FXQ4`) e treemap "squarified" desenhado em `canvas` do GPUI | — | Disco sem o serviço: cair para os totais do `dir_sizes` (mais lento, com progresso real) |
| T6.2 Linha do tempo | O `fx-indexd` já lê o diário USN; guardar as últimas N mudanças num anel e responder por intervalo | — | Memória do serviço (P9): anel limitado (ex.: 200 mil entradas) |
| T6.3 Buscas vivas | Filtros e busca existentes; buscas salvas no `settings.toml`, atualizadas pelo USN (mesma mensagem da T6.2) | — | Muitas buscas abertas: atualizar só a visível |
| T6.4 Duplicados | Candidatos por tamanho no índice; SHA-256 com BCrypt (`fx_shell::net::sha256`) em thread própria, com progresso real e cancelamento | — | Disco inteiro: limitar por tamanho mínimo e permitir escolher a pasta |
| T6.5 "Ir para" que aprende | Contagem de uso e recência por pasta no SQLite de estado; ordenação "frecency" na paleta | — | Nenhum relevante |
| T6.6 Desfazer entre sessões | Histórico do `fx-ops` serializado no SQLite; o desfazer já nunca sobrescreve | — | Arquivos mudados desde então: o trabalho reverso falha com mensagem, não sobrescreve |
| T6.7 Compactados como pastas | Navegar e extrair pelo próprio shell do Windows (pasta compactada para ZIP; 7z, RAR e tar no Windows 11 23H2+) via `fx-shell`; a listagem de ZIP já existe (`zip_list`) | Só se for preciso ler 7z/RAR no Windows 10 | Caminhos maliciosos no arquivo: extração sempre confere o destino |
| T6.8 Busca no conteúdo | Primeiro o Windows Search (já indexa conteúdo onde o usuário ativou), com aviso quando a pasta não está indexada | Índice próprio (ex.: `tantivy`) só se o do Windows não bastar | Memória (P8) e tempo de indexação |
| T6.9 + T6.10 Detalhes e colunas | Property System do Windows (`IPropertyStore`, `SHGetPropertyStoreFromParsingName`) no `fx-shell`, em thread própria e sob demanda: dimensões, duração, EXIF, autor | — | Leitores de propriedade de terceiros lentos: tempo limite, como no menu |
| T6.11 Comparar pastas | Listagens que já existem, comparação por nome, tamanho e data; sincronizar pelo `fx-ops` (com desfazer) | — | Pastas enormes: comparar em lotes, com progresso real |
| T6.12 Status do Git | `git status --porcelain=v2 -z` em thread própria quando o `git` existe; sem ele, nada aparece | — | Repositórios grandes: tempo limite e cache por pasta |
| T6.13 Espaços de trabalho | A sessão já salva janelas e abas; espaços são sessões com nome, trocadas pela paleta | — | Nenhum relevante |
| T6.14 Abas entre janelas | Janelas múltiplas já existem; arrastar a aba para fora cria a janela e move o estado | — | API de arrastar entre janelas do GPUI: protótipo antes |
| T6.15 Busca por significado | — | Windows App SDK (APIs de IA do Windows) | Só em PCs Copilot+; opcional e desligada por padrão |
| T6.16 Ações rápidas | Imagens com o WIC do Windows (converter, redimensionar, girar); renomear pela data EXIF com a T6.9 | Juntar PDFs exige uma biblioteca de PDF (ex.: `lopdf`) | Arquivos grandes: sempre gera cópias, nunca altera o original |

**Mudanças no plano:** T6.9 e T6.10 viram uma entrega só (a mesma fonte de dados). T6.2 e T6.3 compartilham a mensagem de mudanças recentes do `fx-indexd`. A ordem passa a começar pelo que não depende de nada novo e tem maior retorno por esforço.

## Ordem recomendada

Revista na D-068 (antes: mapa, linha do tempo e buscas vivas primeiro):

1. **Base rápida, sem dependência nova:** T6.5 "Ir para" que aprende, T6.13 Espaços de trabalho, T6.6 Desfazer entre sessões.
2. **Diferenciais do índice:** T6.1 Mapa do disco, T6.2 Linha do tempo e T6.3 Buscas vivas (mensagem nova do `fx-indexd`), T6.4 Duplicados.
3. **Lacunas que fazem voltar ao Explorador:** T6.9 + T6.10 Detalhes e colunas (Property System), T6.7 Compactados pelo shell.
4. **Fluxos avançados:** T6.11 Comparar pastas, T6.12 Status do Git, T6.16 Ações rápidas (imagens; PDF só com aprovação da dependência), T6.14 Abas entre janelas (protótipo antes).
5. **Opcionais com dependência nova:** T6.8 Busca no conteúdo (Windows Search antes de índice próprio), T6.15 Busca por significado.

## Portão da Fase 6

Os diferenciais do grupo A entregues e conferidos com o usuário; todas as metas de `docs/PERFORMANCE.md` dentro do limite.
