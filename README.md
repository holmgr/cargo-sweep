# Cargo-sweep

A tool for cleaning unused build files created by Cargo/rustc.

## Quick start

To install run:

```
cargo install cargo-sweep
```

To clean all build files older than 30 days in the local cargo project run:

```
cargo sweep -t 30
```

You can also specify a path instead of defaulting to the current directory:

```
cargo sweep -t 30 <path>
```

Finally, you can recursivly clean all cargo project below a given path by adding the `-r` flag, for instance:

```
cargo sweep -r -t 30 code/github.com/holmgr/
```

For more information run:

```
cargo sweep -h
```

## License

Cargo-sweep is distributed under the terms the MIT license.

See [LICENSE](LICENSE) for details.
