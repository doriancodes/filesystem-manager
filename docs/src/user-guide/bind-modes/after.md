# After Mode

The After mode appends the new source to the mountpoint's search path, making it a fallback option.

## Usage

```shell
frg bind -a src mountpoint
```

## Behavior

Frogger resolves lookups by searching the original `mountpoint` first, and if the file isn't found there, it checks the `src`. This maintains existing content's priority while providing additional fallback options.

## Examples

### Additional Fonts

Add extra fonts while maintaining system defaults:

```shell
frg bind -a /extra/fonts /fonts
```

System fonts remain the primary source, with additional fonts available when needed.

### Configuration Extensions

Add supplementary configuration files:

```shell
frg bind -a /additional/config /etc
```

This ensures `/etc` retains its default behavior but gains additional configuration files when defaults don't exist.
