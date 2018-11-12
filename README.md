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

To preview the results of a sweep run add the `-d` flag, for instance:

```
cargo sweep -d -t 30
```

To clean everything but the latest build you will need to run it in several steps
```
cargo sweep -s

<Insert cargo-build, cargo test etc...>

cargo sweep -f
```
The first step generates a timestamp file which will be used to clean everything that was not used between it and the next time the file (-f) option is used.

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
