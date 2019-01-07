cargo clean
cargo +nightly build
cargo +stable build
cargo sweep -s
cargo +stable build
cargo sweep -f
cargo +stable build
cargo +nightly build
