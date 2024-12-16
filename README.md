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
Bind a source directory to a target directory (creates a new session)
- Options:
  - `-b, --before`: Bind before existing bindings (default)
  - `-a, --after`: Bind after existing bindings
  - `-r, --replace`: Replace existing bindings
  - `-c, --create`: Create new binding
  - `-v, --verbose`: Enable verbose logging

#### mount
Mount a directory to a mount point (creates a new session)
- Options:
  - `--node-id <ID>`: Node identifier (defaults to "localhost")
  - `-v, --verbose`: Enable verbose logging

#### session
Manage filesystem sessions
- Options:
  - `-l, --list`: List all active sessions
  - `-k, --kill <session-id>`: Kill a specific session
  - `-p, --purge`: Kill all active sessions
  - `-v, --verbose`: Enable verbose logging
- Usage:
  - `session <session-id>`: Show detailed information for a specific session

### Examples:

```bash
# Bind Operations (creates a new session)
frg bind /source/dir /target/dir              # Bind with default mode (before)
frg bind -a /source/dir /target/dir           # Bind after existing bindings
frg bind -r /source/dir /target/dir           # Replace existing bindings
frg bind -c /source/dir /target/dir           # Create new binding
frg bind -v /source/dir /target/dir           # Bind with verbose logging

# Mount Operations (creates a new session)
frg mount /source/dir /mount/point            # Mount with default node-id
frg mount /source/dir /mount/point mynode     # Mount with custom node-id
frg mount -v /source/dir /mount/point         # Mount with verbose logging

# Session Management
frg session -l                              # List all active sessions
frg session --list                          # List all active sessions
frg session -k abc123                       # Kill session abc123
frg session --kill abc123                   # Kill session abc123
frg session -p                              # Kill all active sessions
frg session --purge                         # Kill all active sessions
frg session abc123                          # Show details for session abc123
```

For detailed help on any command, use:
```bash
frg --help                  # General help
frg <command> --help        # Command-specific help
```

## License

BSD-3-Clause

