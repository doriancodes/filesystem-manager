# Frogger 🐸

Simple file system implementation using the 9P protocol

Frogger is a modern implementation of the Plan 9 filesystem protocol (9P), focusing on providing flexible namespace management through bind operations.

## What is 9P?

The 9P protocol is a network protocol developed for the Plan 9 operating system, designed to provide transparent access to resources across a network. One of its key features is the ability to manipulate the namespace through operations like `bind`.

## Key Features

- **Flexible Namespace Management**: Modify your filesystem view without affecting the underlying system
- **Multiple Binding Modes**: Support for Replace, Before, After, and Create modes
- **Union Directories**: Combine multiple directories into a single view
- **Custom Environments**: Create isolated filesystem environments

## Quick Links

- [Getting Started](user-guide/getting-started.md)
- [Bind Operations](user-guide/bind.md)
- [CLI Reference](reference/cli.md)

## Documentation

This documentation is available online at [frogger.github.io/frogger](https://frogger.github.io/frogger) and can be built locally using [mdBook](https://rust-lang.github.io/mdBook/). To build and serve the documentation locally:

```bash
mdbook serve docs/
```

This will serve the documentation at [localhost:3000](http://localhost:3000).
