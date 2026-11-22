# Assinatura digital (Fase 5, T5.2)

Sem assinatura, o Windows mostra "editor desconhecido" (SmartScreen) ao abrir o instalador ou o `fx.exe` baixados. Para projetos open source, a **SignPath Foundation** assina de graça com um certificado dela.

## O que só o usuário pode fazer

1. Inscrever o projeto em <https://signpath.org/apply> (repositório público `heapwraith/fx`, licença MIT OR Apache-2.0, releases pelo GitHub Actions).
2. Depois de aprovado: no painel da SignPath, criar o projeto `fx` com uma *signing policy* `release-signing` e um *artifact configuration* que aceite o zip de release (assinar `fx.exe`, `fx-indexd.exe`, `fx-preview.exe`, `fx-setup.exe` e o instalador gerado com `fx-setup --pack`).
3. No GitHub (Settings → Secrets and variables → Actions), criar o segredo `SIGNPATH_API_TOKEN` e as variáveis `SIGNPATH_ORGANIZATION_ID` e `SIGNPATH_PROJECT_SLUG`.

## O que o agente faz depois

- Acrescentar ao job `release` do `.github/workflows/ci.yml` o envio dos executáveis para a SignPath (ação oficial `signpath/github-action-submit-signing-request`) antes de montar o zip e o instalador, só quando o segredo existir; o instalador é gerado com os executáveis já assinados e então ele mesmo é assinado. Com o fx assinado, o atualizador passa a exigir o instalador assinado (D-056).
- Conferir no release seguinte que as Propriedades → Assinaturas Digitais mostram a SignPath Foundation.

Enquanto isso, os releases continuam saindo sem assinatura (marcados como pré-lançamento), e o README explica como passar pelo aviso.
