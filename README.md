---
runme:
  id: 01HHK4WTYBBADD9AQ2CCEYFP5F
  version: v2.0
---

# neutron-contracts

## Development

### Environment Setup

- Rust v1.71.0+
- `wasm32-unknown-unknown` target
- Docker

1. Install `rustup` via <https://rustup.rs/>
2. Run the following:

```sh {"id":"01HHK4WTYBBADD9AQ2C709HRAG"}
rustup default stable
rustup target add wasm32-unknown-unknown
```

3. Make sure [Docker](https://www.docker.com/) is installed

### Unit Tests

Each contract contains Rust unit tests embedded within the contract source directories. You can run:

```sh {"id":"01HHK4WTYBBADD9AQ2C94SHQGV"}
make test
```

### Generating schema

```sh {"id":"01HHK4WTYBBADD9AQ2C9B6886V"}
make schema
```

### Production

For production builds, run the following:

```sh {"id":"01HHK4WTYBBADD9AQ2CBHF9BP8"}
make build
```

This performs several optimizations which can significantly reduce the final size of the contract binaries, which will be available inside the `artifacts/` directory.
