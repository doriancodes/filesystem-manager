# CLI Reference

Complete reference for the Frogger command-line interface.

## Global Options

```shell
frg [OPTIONS] <COMMAND>
```

### Options
- `-v, --verbose`: Enable verbose logging
- `-q, --quiet`: Suppress all output except errors
- `-h, --help`: Show help information

## Commands

### bind

Bind a source directory to a mountpoint.

```shell
frg bind [OPTIONS] <SOURCE> <MOUNTPOINT>
```

#### Options
- `-b, --before`: Bind source before existing content
- `-a, --after`: Bind source after existing content
- `-c, --create`: Create mountpoint if it doesn't exist
- `-r, --recursive`: Recursively bind subdirectories

#### Examples
```shell
# Replace binding
frg bind /source /dest

# Before binding with mountpoint creation
frg bind -b -c /custom/bin /opt/tools

# After binding
frg bind -a /fallback/config /etc
```
