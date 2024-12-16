# Installation

## System Requirements

- Unix-based operating system (Linux, macOS)
- Linux kernel 4.18 or later
- FUSE filesystem support

## Installing FUSE

Before installing froggr, you need to install FUSE on your system:

### Ubuntu/Debian
```bash
sudo apt-get install fuse
```

### macOS
```bash
brew install macfuse
```
Note: On macOS, you may need to allow the system extension in System Preferences > Security & Privacy after installing macFUSE.

### Fedora
```bash
sudo dnf install fuse
```

### Arch Linux
```bash
sudo pacman -S fuse
```

## From Source

### Prerequisites
- Rust toolchain (1.70 or later)
- Cargo package manager
- FUSE installed (see above)

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
