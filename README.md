# froggr 🐸

Simple file system implementation using the 9P protocol

## User Guide

The user guide is available online at [doriancodes.github.io/froggr](https://doriancodes.github.io/froggr/) and can be built locally using [mdBook](https://rust-lang.github.io/mdBook/). To build and serve the documentation locally:

```bash
mdbook serve docs/
```

This will serve the documentation at [localhost:3000](http://localhost:3000).

### Commands:

#### bind
Bind a source directory to a target directory
- Options:
  - `-b, --before`: Bind before existing bindings (default)
  - `-a, --after`: Bind after existing bindings
  - `-r, --replace`: Replace existing bindings
  - `-c, --create`: Create new binding
  - `-v, --verbose`: Enable verbose logging

#### mount
Mount a directory to a mount point
- Options:
  - `--node-id <ID>`: Node identifier (defaults to "localhost")
  - `-v, --verbose`: Enable verbose logging

#### session
Start a new session daemon
- Options:
  - `-r, --root <PATH>`: Root directory for the session (defaults to current directory)
  - `--pid-file <PATH>`: Custom PID file location
  - `--privileged`: Run with elevated privileges (requires root)
  - `-v, --verbose`: Enable verbose logging

### Examples:

```bash
# Bind Operations
frg bind /source/dir /target/dir              # Bind with default mode (before)
frg bind -a /source/dir /target/dir           # Bind after existing bindings
frg bind -r /source/dir /target/dir           # Replace existing bindings
frg bind -c /source/dir /target/dir           # Create new binding
frg bind -v /source/dir /target/dir           # Bind with verbose logging

# Mount Operations
frg mount /source/dir /mount/point            # Mount with default node-id
frg mount /source/dir /mount/point mynode     # Mount with custom node-id
frg mount -v /source/dir /mount/point         # Mount with verbose logging

# Session Operations
frg session                                   # Start session in current directory
frg session -v                                # Start session with verbose logging
frg session --root /path/to/dir               # Start session in specific directory
sudo frg session --privileged                 # Start privileged session
frg session --pid-file /path/to/pid           # Start session with custom PID file
```

For detailed help on any command, use:
```bash
frg --help                  # General help
frg <command> --help        # Command-specific help
```

## License

BSD-3-Clause

