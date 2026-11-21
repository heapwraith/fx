Pegue a próxima tarefa não concluída (`- [ ]`) do arquivo da fase atual em `docs/phases/` (a de menor número que ainda tem tarefas abertas).

1. Leia a tarefa inteira, os critérios de aceite e as seções de `docs/ARCHITECTURE.md`, `docs/PERFORMANCE.md` e `docs/SAFETY.md` que ela toca.
2. Antes de escrever código, me diga em até 5 linhas: o que vai fazer, quais arquivos vai criar ou alterar e como vai provar cada critério de aceite. Se algo estiver ambíguo, pergunte aqui e espere.
3. Implemente a menor solução que cumpre os critérios.
4. Rode `cargo fmt --all`, `cargo xwin clippy --target x86_64-pc-windows-msvc --workspace --all-targets -- -D warnings` e `cargo xwin test --workspace` (ver D-010). Se a tarefa tem meta de desempenho, rode o benchmark em `--release`.
5. Marque a tarefa como `[x]` com a data e um resumo de uma linha incluindo os números medidos. Atualize `docs/DECISIONS.md` se a tarefa pede.
6. Faça um commit: `T<id>: <resumo>`.
7. Pare e me mostre o resultado. Não comece a tarefa seguinte sem eu pedir.
