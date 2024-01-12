---
runme:
  id: 01HJFZ3QYTBN64SW4CT9JH4EDP
  version: v2.0
---

# CW20 rToken

This is a basic implementation of a cw20 contract. It implements
the [CW20 spec](../../packages/cw20/README.md) and is designed to
be deployed as is, or imported into other contracts to easily build
cw20-compatible tokens with custom logic.

Implements:

- [x] CW20 Base
- [x] Mintable extension
- [x] Allowances extension

## Running this contract

You will need Rust 1.44.1+ with `wasm32-unknown-unknown` target installed.

You can run unit tests on this via:

`cargo test`

Once you are happy with the content, you can compile it to wasm via:

```sh {"id":"01HJFZ3QYTBN64SW4CT5M6YNSY"}
RUSTFLAGS='-C link-arg=-s' cargo wasm
cp ../../target/wasm32-unknown-unknown/release/lsd_token.wasm .
ls -l lsd_token.wasm
sha256sum lsd_token.wasm
```