# Create Mode

The Create mode ensures the mountpoint exists before performing the bind operation.

## Usage

```shell
frg bind -c src mountpoint
```

## Behavior

Before performing the bind operation, Frogger checks if the mountpoint exists. If it doesn't, the directory is created automatically. After creation (if needed), it performs a standard replace binding.

## Examples

### Log Directory Setup

Create and bind a custom log directory:

```shell
frg bind -c /data/logs /var/log/app
```

This creates `/var/log/app` if it doesn't exist, then binds `/data/logs` to it.

### Development Environment

Set up a new development environment:

```shell
frg bind -c /dev/workspace/bin /opt/tools
```

Creates `/opt/tools` if needed, then binds the workspace binaries.
