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
    src/parser    SQL lexer, AST, recursive-descent parser
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

- **Limitations.**
  - INT keys only; one indexed column per table.
  - The tree is not maintained on `delete` or a key-changing `update`.
    `find_by_key` re-reads the row through `get` and checks the key, so stale
    entries resolve to `None` rather than a wrong row — but freed entries are
    not reclaimed.

## Parser

A hand-written lexer + recursive-descent parser (no external crate). It turns a
query string into an internal AST (`src/parser/ast.rs`) and never touches
storage — execution is a later stage.

Supported subset:

    CREATE TABLE t (col INT|TEXT [NOT NULL | NULL], ...)
    INSERT INTO t [(col, ...)] VALUES (v, ...)        -- v: int | 'string' | NULL
    SELECT */col,... FROM t [WHERE expr] [ORDER BY col [ASC|DESC]] [LIMIT n]

`expr` is built from column refs and int/string/NULL literals with `= != < <= >
>=`, combined by `AND`/`OR` (AND binds tighter) and parentheses.

AST shape: `Statement` is `CreateTable { table, columns }`, `Insert { table,
columns, values }`, or `Select { projection, from, filter, order_by, limit }`.

It does **not** check that tables/columns exist, that types are compatible, or
that value and column counts match — that belongs to a later binder. Invalid
input yields a positioned error (`parse error at position N: ..., found ...`)
and never panics.

Run it:

    cargo run -- parse --query "SELECT id, name FROM users WHERE age > 18"

