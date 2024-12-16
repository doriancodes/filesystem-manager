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

#### sessions
List all active sessions
- Shows session ID, PID, root directory, and active mounts/binds

#### kill
Terminate a specific session
- Arguments:
  - `session-id`: The ID of the session to terminate

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
frg sessions                                  # List all active sessions
frg kill <session-id>                         # Terminate a specific session
```

For detailed help on any command, use:
```bash
frg --help                  # General help
frg <command> --help        # Command-specific help
```

## License

BSD-3-Clause

