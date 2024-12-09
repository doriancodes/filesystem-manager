# Frogger 🐸

Simple file system implementation using the 9P protocol

## Bind
The P9 protocol for file systems requires a unique feature called `bind`, which allows for flexible control over the namespace
(file hierarchy). The bind operation maps a file, directory, or another namespace tree into a new location in the namespace.
It supports three binding modes: Before, After, and Replace. Each mode provides different behaviors for resolving file
lookups when multiple resources are mapped to the same namespace.

### Replace Binding: `bind src mountpoint`
This mode replaces whatever was previously mounted at the mountpoint with the src. Only the new src is visible at the specified
mountpoint.
- **Behavior**: The `src` completely overrides any existing content at the `mountpoint`.
- **Example use cases**:
  - Temporarily replacing a default configuration directory with a test or alternative version
  ```shell
    bind /test/config /etc
  ```
  After this, processes see `/test/config` contents instead of the original `/etc`.
  - Redirecting access to `/bin` to a custom toolchain directory for development:
  ```shell
    bind /custom/tools/bin /bin
  ```

### Before binding `bind -b src mountpoint`
In this mode the `src` is placed *before* the existing contents of the mountpoint. When a lookup occurs,
a Plan9 file system searches `src` first, and if the file isn't found there, it searches the original `mountpoint`.
- **Behavior**: Adds `src` at a higher priority, leaving the existing content accessible as a fallback.
- **Use case**:
  - Overlaying new tools or files over existing directories without completely replacing them. For example,
  adding custom binaries that take precedence over system binaries:
  ```shell
    bind -b /custom/bin /bin
  ```
  In this case, `/custom/bin/ls` will be used instead of `/bin/ls` if both exist.
- **Example**: Temporarily prioritizing a new set of libraries or data over the default paths for testing or debugging.

### After binding `bind -a src mountpoint`
This mode appends the `src` to the `mountpoint`'s search path. Plan 9 resolves lookups by searching the original
`mountpoint` first, and if the file isn't found there, it checks the `src`.
- **Behavior**: Adds `src` as a fallback while maintaining the existing content's priority.
- **Use case**:
  - Adding extra directories to extend a namespace without interfering with its current operation. For example, appending a directory with additional fonts:
  ```shell
    bind -a /extra/fonts /fonts
  ```
  Here, `/fonts` will use default system fonts first and fall back to `/extra/fonts` if needed.
- **Example**: Supplementing a default configuration directory with additional files:
  ```shell
    bind -a /additional/config /etc
  ```
  This ensures `/etc` retains its default behavior but gains the additional configuration files if the defaults don’t exist.

### Union directories
Using `bind -b` and `bind -a`, you can create union directories where files from multiple sources appear merged.
For example:
```shell
bind -b /local/bin /bin
bind -a /backup/bin /bin
```
This setup prioritizes `/local/bin`, followed by `/bin`, and finally `/backup/bin`.

### Custom environments
For isolating environments, such as creating chroot-like environments or managing per-process views of namespaces.
