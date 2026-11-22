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

## Ordem recomendada

1. T6.1 Mapa do disco, T6.2 Linha do tempo, T6.3 Buscas vivas — três diferenciais únicos que saem do índice e do USN.
2. T6.7 Arquivos compactados, T6.9 Painel de detalhes — as lacunas que mais fazem alguém voltar ao Explorador.
3. T6.4 Duplicados, T6.5 "Ir para" que aprende, T6.6 Desfazer entre sessões.
4. T6.8 Busca no conteúdo e o restante do grupo B, conforme o uso pedir.
5. Grupo C, como opcional.

## Portão da Fase 6

Os diferenciais do grupo A entregues e conferidos com o usuário; todas as metas de `docs/PERFORMANCE.md` dentro do limite.
