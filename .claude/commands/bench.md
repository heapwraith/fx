Rode os benchmarks e compare com as metas.

1. `cargo xwin run -p fx-bench --release -- fixtures` (só gera se faltar).
2. `cargo xwin run -p fx-bench --release -- all`. Benchmarks que precisam de Administrador (índice da MFT): se o terminal não estiver elevado, pule-os e diga quais ficaram de fora.
3. Leia os resultados mais recentes em `bench/results/` e mostre uma tabela: ID da meta (P1–P9), meta, mediana, p95, passou?
4. Compare com a execução anterior, se houver, e destaque regressões acima de 10%.
5. Não altere código neste comando.
