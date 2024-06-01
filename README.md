# Internal marketmaking bot

```bash
# Static production build against musl. Outputs are at `result`.
nix build

# Development build.
cargo build

# Run one of the executables, e.g. the `market_maker` executable.
cargo run --bin market_maker

# Run the tests.
nix flake check -L

# Run pre-commit hooks manually.
pre-commit run -a

# Build the container and copy it to the local docker daemon.
# (Needs custom entrypoint settings)
nix run .#image.copyToDockerDaemon
```

## hyperliquid-rust-sdk

SDK for Hyperliquid API trading with Rust.

## Usage Examples

See `src/bin` for examples. You can run any example with `cargo run --bin [EXAMPLE]`.

## Installation

`cargo add hyperliquid_rust_sdk`

## License

This project is licensed under the terms of the `MIT` license. See [LICENSE](LICENSE.md) for more details.

```bibtex
@misc{hyperliquid-rust-sdk,
  author = {Hyperliquid},
  title = {SDK for Hyperliquid API trading with Rust.},
  year = {2023},
  publisher = {GitHub},
  journal = {GitHub repository},
  howpublished = {\url{https://github.com/hyperliquid-dex/hyperliquid-rust-sdk}}
}
```

## Terms

By using this package you agree to the Terms of Use. See [TERMS](TERMS.md) for more details.
