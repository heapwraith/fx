---
name: fx-brand
description: Identidade da marca Pathdash (o produto do repositório fx): nome, ícone, cores, slogan, voz e como criar ou mudar qualquer peça de marca. Use sempre que a tarefa tocar o ícone do app, logotipo, nome do produto em textos, instalador, release, winget, README, site, capturas de divulgação, ou quando o usuário pedir nomes, marcas, ícones ou "identidade", mesmo que não diga "marca". Também traz o roteiro para verificar nomes (apps, domínios, INPI, USPTO) antes de apresentá-los.
---

# fx-brand — identidade Pathdash

O produto se chama **Pathdash** (D-074). Por dentro o projeto continua **fx** (crates `fx-*`, pastas `%APPDATA%\fx`, registro, serviço `fx-indexd`, repositório `heapwraith/fx`): essas partes não mudam sem plano de migração, porque o atualizador das versões instaladas depende delas. A interface segue a skill `fx-design`; esta trata do que é marca.

## 1. Nome

- Escreve-se **Pathdash** (P maiúsculo, uma palavra). Nunca "PathDash", "PATHDASH" nem "Path Dash" em texto. Em minúsculas só no logotipo, nos nomes de arquivo (`pathdash.exe`) e em identificadores.
- Em português é masculino: "o Pathdash", "do Pathdash", "abrir o Pathdash".
- Significado: *path* (caminho de uma pasta) + *dash* (arrancada). Pode ser usado como verbo em tom leve ("pathdash pra pasta do projeto"), nunca em texto de interface.
- Arquivos públicos: `Pathdash-<versão>-setup-x64.exe`, `Pathdash-<versão>-windows-x64.zip`. O atualizador escolhe o instalador pelo final `-setup-<arch>.exe`: não mude esse final.

## 2. Slogan e voz

- Slogan: **"Qualquer pasta, numa arrancada."** / **"Every path, one dash away."** Fonte única: `slogan()` em `crates/fx-ui/src/tabs_view/guide.rs` (o instalador repete o mesmo texto).
- Frase de apoio (README, lojas): "Pastas abrem na hora e arquivos aparecem enquanto você digita."
- Voz: direta, rápida, sem exagero e sem ponto de exclamação. Números reais em vez de adjetivos ("3 arquivos em 4 ms", nunca "super rápido"). Os textos de interface seguem a seção 3 da `fx-design`.
- Nada de status inventado: todo número que a marca mostra vem de medição (ver `docs/PERFORMANCE.md`).

## 3. Ícone

Fonte da verdade: `tools/make-icon.py` (Pillow + numpy). Ele gera `crates/fx-ui/resources/fx.ico` (16–256 px) e `logo-128.png`; os três programas embutem esse `.ico`. Rodar `python3 tools/make-icon.py <pasta>` também grava as prévias e uma folha em fundo escuro e claro.

Construção, numa grade de 256 unidades:
- **Placa:** superelipse de 16 a 240 em degradê grafite na diagonal (`#4A505A` → `#262A31` → `#101215`), com brilho branco suave vindo do canto superior esquerdo.
- **Desenho:**
  - um anel em (58,176), o "você está aqui", com aro `#5FE6F2`;
  - dele sai um caminho ciano `#2FD9E8` de 20 unidades de espessura, mais transparente no começo e opaco no fim, que termina numa seta de 26 unidades em (150,86)–(196,128)–(150,170);
  - dois traços apagados no alto marcam a velocidade.
- **Luz:** sombra só sob o desenho, com deslocamento de 7 e desfoque de 7; reflexo no topo; borda interna clara em cima e escura embaixo.
- **Até 24 px**, desenho próprio: placa lisa `#22262C` com aro claro, caminho mais grosso (30) e seta (38) em `#7DEEF7`, sem anel nem efeitos.

Regras:
- Mudou o desenho? Altere o script, gere de novo, abra a folha e confira em zoom 16, 24 e 32 px (sem serrilhado, seta legível). Faça o commit do `.ico` e do PNG junto com o script.
- Traços sempre com o "pincel" redondo do script, nunca com `ImageDraw.line` com junções: as junções deixam emendas visíveis.
- Sem pasta, lupa ou bicho no ícone: a marca é o caminho e a seta.

## 4. Cores

- Cor da marca: **ciano**. No app ele é o único destaque: `accent` `#0A6C8A` no claro e `#4CD7F0` no escuro (tokens em `crates/fx-theme/src/lib.rs`, com testes de contraste). O grafite fica só no ícone.
- Os temas claro e escuro (fundos e textos) não mudam por causa de marca.
- O app abre no escuro por padrão; o usuário escolhe em Configurações › Aparência.
- Cor nova em peça de marca: calcule o contraste antes (≥ 4,5:1 para texto, ≥ 3:1 para elementos gráficos) e registre-o.

## 5. Onde a marca aparece (lista de conferência)

Ao mudar nome, ícone ou slogan, passe por todos:
- **Interface:** título da janela, "Sobre", "Como usar", paleta, Configurações e avisos (`tr!`).
- **Programas:** Propriedades dos `.exe` (os `build.rs` de `fx-ui`, `fx-setup` e `fx-setup-boot`) e o nome exibido do serviço (`fx-index`).
- **Instalador:** telas, atalho `Pathdash.lnk` e entrada em Configurações › Aplicativos (`crates/fx-setup/src/transaction.rs`).
- **Distribuição:** CI (nomes do release e título), `tools/winget-manifest.py` e `tools/third-party-notices.py` (gerar o arquivo de novo).
- **Documentos:** `README.md`, `docs/GUIA.md`, `CHANGELOG.md` e a decisão em `docs/DECISIONS.md`.
- **Conferência:** capture a janela real (skill `fx-design`, seção 5) para ver o título e a tela "Sobre".

## 6. Propor nomes ou marcas novas

Pesquise antes de apresentar, sempre, e traga só o que passou, com o resultado da busca e uma linha por descartado:
1. **Apps e softwares com o mesmo nome:** busca na web pelo nome entre aspas + "app" / "software".
2. **Domínios:** RDAP (`https://rdap.org/domain/<nome>.<tld>`; 404 = livre; repita quando vier 429). Abra os sites registrados para ver se são produtos ativos ou só estacionados. O `dig` não existe neste ambiente.
3. **USPTO:** POST JSON em `https://tmsearch.uspto.gov/prod-stage-v1-0-0/tmsearch` com `{"query":{"bool":{"should":[{"query_string":{"query":"wordmark:<nome>"}}]}},"size":20}`. Rode também uma consulta de controle com uma marca conhecida, para provar que a busca funciona.
4. **INPI:** sessão em `https://busca.inpi.gov.br/pePI/servlet/LoginController?action=login`, depois POST em `MarcasServletController` com `Action=searchMarca`, `marca=<nome>`, `buscaExata=sim|nao`, `tipoPesquisa=BY_MARCA_CLASSIF_BASICA`, em ISO-8859-1. Também com uma consulta de controle.
5. **Classes que importam:** 9 (software) e 42 (serviços de software).

Verificação de 2026-11-23 para "Pathdash": nenhum registro no USPTO (nem para "path dash", "dashpath" e variantes), nenhum no INPI (busca exata e aproximada), nenhum app ou software com esse nome (só o E.PathDash, painel acadêmico em R Shiny). Domínios pathdash.app, .dev e .io livres; .com registrado sem site. Isso não substitui o pedido de registro: o órgão faz a própria busca no exame.
