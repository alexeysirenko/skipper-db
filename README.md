# Skipper-DB

A small row-store DBMS in Rust.

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
    src/catalog   db directory init/open, table schema
    src/storage   record codec, slotted pages, table heap, B+Tree index
    src/types     Value / ColumnType
    src/error     thiserror enum
    tests/        cli integration tests

## Storage engine

Row-store with a SQLite-like binary layout — fixed 4 KiB pages, byte offsets,
no text formats.

- A table is a `.tbl` file: page 0 is metadata (magic, version, page size,
  indexed column, serialized schema); pages 1..N are slotted data pages.
- A record is a null bitmap followed by the non-null values (INT = 8 bytes LE,
  TEXT = length-prefixed UTF-8).
- A row is addressed by a `Rid` (page, slot). `delete` tombstones a slot;
  `update` rewrites in place or relocates within the page.

API: `create` / `open` / `insert` / `get` / `scan` / `update` / `delete`.

## Indexing (B+Tree)

The optional secondary structure is a **B+Tree** index over one **INT** column,
stored in a sibling `<table>.idx` file (page 0 metadata + one node per page).

- **Why it fits a row-store.** The store is tuple-oriented, so a B+Tree gives
  O(log n) point lookup by key and keeps keys ordered, complementing the heap's
  O(n) sequential scan.
- **What it speeds up.** `find_by_key(k)` descends the tree to a `Rid` instead
  of scanning every page; `insert` keeps the tree in sync.
- **Limitations.**
  - INT keys only; one indexed column per table.
  - The tree is not maintained on `delete` or a key-changing `update`.
    `find_by_key` re-reads the row through `get` and checks the key, so stale
    entries resolve to `None` rather than a wrong row — but freed entries are
    not reclaimed.
  - Small fanout (`MAX_KEYS = 4`) for clarity; the on-disk format does not
    depend on it.
