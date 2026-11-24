# ntfs-reader 0.4.6 com patch do fx

Cópia do crate `ntfs-reader` 0.4.6 do crates.io (licença MIT OR Apache-2.0, ver `LICENSE-MIT` e `LICENSE-APACHE`), usada pelo workspace via `[patch.crates-io]` (D-076). Cada trecho alterado tem um comentário `fx patch:`; o original está no crates.io.

| Arquivo | Mudança | Por quê |
|---|---|---|
| `src/aligned_reader.rs` | Leituras de 64 KiB ou mais vão ao volume em pedidos alinhados de até 16 MiB (`read_large`), direto no buffer de quem pediu quando já estão alinhadas. Leituras pequenas seguem o caminho original. O teste `aligned_read_tests` em `crates/fx-index/src/windows/mod.rs` confere que os bytes são os mesmos. | O original lia a MFT inteira em pedidos de 4 KiB, cada um com um seek: centenas de milhares de pedidos para uma MFT de ~1 GB, e o tempo da P6 ficava preso à latência de cada pedido. |

**Para atualizar:** copie a nova versão do registry e repita a mudança (procure `fx patch`).
