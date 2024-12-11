# Replace Mode

The Replace mode is the default binding mode in froggr. It completely replaces whatever was previously mounted at the mountpoint with the new source.

## Usage

```shell
frg bind src mountpoint
```

## Behavior

The `src` completely overrides any existing content at the `mountpoint`. After the bind operation, only the contents of `src` will be visible at the specified mountpoint.

## Examples

### Replacing Configuration Directory

Temporarily replace a default configuration directory with a test version:

```shell
frg bind /test/config /etc
```

After this command, processes will see `/test/config` contents instead of the original `/etc`.

### Custom Toolchain

Redirect access to `/bin` to a custom toolchain directory for development:

```shell
frg bind /custom/tools/bin /bin
```

This replaces the system binaries with your custom tools. 