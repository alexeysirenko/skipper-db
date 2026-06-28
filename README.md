# Skipper-DB

A small row-store SQL database in Rust, driven through a CLI.

## Build

    cargo build

## Run

    cargo run -- query --db ./demo-db --sql "CREATE TABLE users (id INT, name TEXT, age INT)"
    cargo run -- query --db ./demo-db --sql "INSERT INTO users VALUES (1, 'Alice', 20)"
    cargo run -- query --db ./demo-db --sql "SELECT name FROM users WHERE age > 18"

`RUST_LOG=debug` for verbose logs (logs go to stderr, results to stdout).

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

A hand-written lexer + recursive-descent parser that turns a query string into
an internal AST (`src/parser/ast.rs`). It only parses — no storage access, no
name/type checking (that's a later binder).

Supported subset:

    CREATE TABLE t (col INT|TEXT [NOT NULL | NULL], ...)
    INSERT INTO t [(col, ...)] VALUES (v, ...)        -- v: int | 'string' | NULL
    SELECT */item,... FROM t [WHERE expr] [ORDER BY col [ASC|DESC]] [LIMIT n]

A projection item is `expr [AS alias]`. `expr`: columns, literals, the scalar
function `LENGTH(text)`, and comparisons (`= != < <= > >=`) joined by `AND`/`OR`
(AND tighter) with parentheses. Errors are positioned; bad input never panics.

    cargo run -- parse --query "SELECT id, name FROM users WHERE age > 18"

## Planner

Turns an AST into a logical plan (`src/query/plan.rs`) and binds it against the
catalog (table/column existence, value counts). A SELECT becomes a tree built
bottom-up Scan -> Filter -> Sort -> Projection -> Limit:

    Limit 10
      Projection [id, name]
        Filter [age > 18]
          Scan users

It still does not execute — just builds and validates the plan. `--db` points at
a directory of `<table>.tbl` files used as the catalog.

    cargo run -- plan --db ./demo-db --query "SELECT id FROM users WHERE age > 18 LIMIT 5"

## Execution

Lowers the logical plan to a physical plan (`...Exec` nodes) and runs it with
Volcano-style pull operators against storage. The full path:

    SQL -> parse -> AST -> bind -> logical plan -> physical plan -> operators -> rows

`TableScanExec` reads live rows via the storage scan; `FilterExec`,
`ProjectionExec`, `SortExec`, `LimitExec` pull from their child. CREATE TABLE and
INSERT go straight to storage, so data persists across runs. Comparisons on NULL
or mismatched types are false (no three-valued logic); `ORDER BY` sorts NULLs
first.

    cargo run -- query   --db ./demo-db --sql "CREATE TABLE users (id INT, name TEXT, age INT)"
    cargo run -- query   --db ./demo-db --sql "INSERT INTO users VALUES (1, 'Alice', 20)"
    cargo run -- query   --db ./demo-db --sql "SELECT id, name FROM users WHERE age > 18 ORDER BY name LIMIT 10"
    cargo run -- explain --db ./demo-db --sql "SELECT id FROM users WHERE age > 18"
