# gpui_http_client 0.2.2 com patch do fx

Cópia do crate `gpui_http_client` 0.2.2 do crates.io (licença Apache-2.0, ver `LICENSE-APACHE`), usada pelo workspace via `[patch.crates-io]` (D-071). Cada trecho alterado tem um comentário `fx patch:`; o original está no crates.io.

| Arquivo | Mudança | Por quê |
|---|---|---|
| `Cargo.toml` | A dependência `zed-reqwest` saiu. | Ela trazia `hyper`, `rustls`, `ring` e mais 50 crates que o fx nunca usa (o fx não faz HTTP pelo GPUI; o atualizador usa o WinHTTP). O `ring` também impedia a compilação ARM64 no WSL. |
| `src/http_client.rs` | `send_multipart_form` saiu do trait `HttpClient` e das duas implementações, com as importações que só ele usava. | Era a única parte da API que usava tipos do `reqwest`; ninguém no GPUI nem no fx a chama. |
| `src/async_body.rs` | `impl TryFrom<reqwest::Body> for AsyncBody` saiu. | Mesmo motivo. |

**Para atualizar:** copie a nova versão do registry e repita as três mudanças (procure `reqwest`).
