# Bind Operations

The P9 protocol for file systems requires a unique feature called `bind`, which allows for flexible control over the namespace
(file hierarchy). The bind operation maps a file, directory, or another namespace tree into a new location in the namespace.

## Available Modes

froggr supports four binding modes:
- [Replace Mode](bind-modes/replace.md): Replace existing content
- [Before Mode](bind-modes/before.md): Add content with higher priority
- [After Mode](bind-modes/after.md): Add content with lower priority
- [Create Mode](bind-modes/create.md): Create mountpoint if needed

Each mode provides different behaviors for resolving file lookups when multiple resources are mapped to the same namespace. 