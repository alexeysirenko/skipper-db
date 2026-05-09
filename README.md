# Skipper-DB

A small DBMS in Rust.

## Build

    cargo build

## Run

    cargo run -- init --db ./demo-db
    cargo run -- open --db ./demo-db

`RUST_LOG=debug` for verbose logs.

## Tests

    cargo test

## Layout

    src/cli       clap entry, subcommand dispatch
    src/catalog   db directory init/open
    src/storage   binary file header (4 KiB, magic + version + page size)
    src/error     thiserror enum
    tests/        cli integration tests
