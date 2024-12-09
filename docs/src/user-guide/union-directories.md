# Union Directories

Union directories allow you to combine multiple sources into a single view, creating a merged namespace.

## Creating Union Directories

You can create union directories by using multiple bind operations with different modes. The order of operations matters, as it determines the lookup priority.

### Basic Example

```shell
frg bind -b /local/bin /bin
frg bind -a /backup/bin /bin
```

This creates a three-layer union:
1. `/local/bin` (highest priority)
2. Original `/bin` (middle priority)
3. `/backup/bin` (lowest priority)

## Common Use Cases

### Development Environment

Create a layered development environment:

```shell
frg bind -b /dev/override /usr
frg bind -a /dev/fallback /usr
```

This allows:
- Development files to override system files
- System files to serve as the default
- Fallback files for missing components

### Configuration Management

Manage multiple configuration sources:

```shell
frg bind -b /etc/custom /etc
frg bind -a /etc/defaults /etc
```

This creates a hierarchy where:
- Custom configurations take precedence
- System configurations remain as default
- Default configurations serve as fallback
