# Before Mode

The Before mode places the new source *before* the existing contents of the mountpoint in the lookup order.

## Usage

```shell
frg bind -b src mountpoint
```

## Behavior

When a lookup occurs, froggr searches `src` first, and if the file isn't found there, it searches the original `mountpoint`. This creates a layered view where new content takes precedence over existing content.

## Examples

### Custom Binary Directory

Add custom binaries that take precedence over system binaries:

```shell
frg bind -b /custom/bin /bin
```

In this case, `/custom/bin/ls` will be used instead of `/bin/ls` if both exist, while other commands will fall back to `/bin`.

### Development Libraries

Prioritize development versions of libraries:

```shell
frg bind -b /dev/libs /usr/lib
```

This allows testing new library versions while maintaining access to the system libraries as fallback.
