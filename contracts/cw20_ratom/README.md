# CW20 rATOM

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

```
RUSTFLAGS='-C link-arg=-s' cargo wasm
cp ../../target/wasm32-unknown-unknown/release/cw20_ratom.wasm .
ls -l cw20_ratom.wasm
sha256sum cw20_ratom.wasm
```