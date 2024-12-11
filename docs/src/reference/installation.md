# Installation

## From Source

### Prerequisites
- Rust toolchain (1.70 or later)
- Cargo package manager

### Steps

1. Clone the repository:
```shell
git clone https://github.com/yourusername/froggr.git
cd froggr
```

2. Build and install:
```shell
cargo install --path .
```

This will install the `frg` binary to your cargo bin directory (usually `~/.cargo/bin`).

## Using Cargo

Install directly from crates.io:

```shell
cargo install froggr
```

## Verifying Installation

After installation, verify that froggr is working correctly:

```shell
frg --version
```

## System Requirements

- Linux kernel 4.18 or later
- FUSE filesystem support
