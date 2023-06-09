# SAT solver ANalyzer

An attempt at building a toolbox for analyzing performance and runtime characteristics of SAT solvers.
This project is currently in its initial development stage.

In collaboration with and initiated by Benjamin Kaiser and Robert Clausecker.
Developed by Cobalt.

## Current state

- Metrics database:
  - duckdb (direct and batched)
  - SQLite
  - (planned, most likely in clustered scenarios) ClickHouse OR PostgreSQL
  - (planned-feature): Merging, i.e., take multiple metric sets and compile into single database
- config:
  - YAML, able to express executors, sets of solvers and sets of test sets (see below)
- executors:
  - local parallel executor: Supervises locally spawned SAT solvers with a thread pool ([rayon](https://github.com/rayon-rs/rayon) based, configurable concurrency, supports thread pinning)
    - The executor only parallelizes the actual execution of the tests, i.e., it is parallel on the data level. This means that the initial process of finding the tests and preparing the data for the solvers may be bound by a single thread. This may be changed in the future but is sufficient for the current test suites.
    - (planned, WIP) SLURM
- tests:
  - tests are grouped in tests sets and identified as files via a [glob](https://github.com/BurntSushi/ripgrep/tree/master/crates/globset) that may be searched within path(s) with [ignore](https://github.com/BurntSushi/ripgrep/tree/master/crates/ignore).
  - Test sets that are supersets of other sets, i.e., test set c with tests from set a and b.
  - GBD collector coming soonish
- ingest:
  - (WIP) minisat and cadical are planned as a first step

## Runtime behavior (WIP)

### Runner

1. Read & Deserialize config
2. Check for consistency etc., so called pre-flight checks
3. Construct Tasks, i.e., figure out what tests exists and map them to solvers
4. Start executing
5. (if necessary), teardown of stateful components like database connections

## Building

### Runner

Required dependencies:

- A rust toolchain (MSRV: `stable` - 1.68.2), recommended: [rustup](https://rustup.rs/)
- A C compiler (symlinked to `cc`) for bundled [DuckDB](https://github.com/duckdb/duckdb), needs to be in `PATH` while `cargo` builds the runner
- OpenSSL development file, usually found in package repositories as `libssl-dev` or `openssl-devel`
- (optional) DuckDB header files, may be found in your package repositories as `duckdb-dev`

Building:

- To create a debug build with the system DuckDB (placed in `target/debug/satan-runner`): `cargo b cargo b --package satan-runner`
- To create a debug build (placed in `target/debug/satan-runner`): `cargo b cargo b --package satan-runner --features bundled-duckdb`
- To create a release, i.e. optimized, build (placed in `target/release/satan-runner`): `cargo b cargo b --package satan-runner --release`
- To create a release with the system DuckDB, i.e. optimized, build (placed in `target/release/satan-runner`): `cargo b cargo b --package satan-runner --release --features bundled-duckdb`

## Config

> The configuration is done in [YAML](https://yaml.org/), other languages may also be supported in the future.

```yaml
# Executor, variant select with YAML tag:
# Support variants:
# - Local
#   - pinned: bool -> pin threads to logical CPUs (default: false)
#   - threads: integer -> number of threads in tread pool (default: number of logical CPUs) }
# - Distributed: A wrapper for distributed usage of the local executor
#   - synchronization: tagged enum (see below) ->
#      - Coordinated: Elect a leader runner and let it coordinate work over MPI, this node may also process tasks at the same time
#      - FilesystemLocks: Use SQLite with filesystem locks to coordinate the work queue. This method relies on the filesystem to handle file locking and employs a temporary SQLite database for distributing work
#        - path: string -> path to SQLite database (must be prepared beforehand AND be available on all compute nodes)
executor: !Local
  pinned: true

# Configuration for metric storage driver, the same tag handling applies here too
# - DuckDB: Uses DuckDB, an sqlite-like file based database, for storage. Recommended for local setups.
#           This driver is, like SQLite, limited to one write at time and works with an internal Mutex.
#   - path: string -> path to duckdb file
# - SQLite: Uses SQLite for storage. Recommended for local setups with high iteration count.
#   - path: string -> path to sqlite file
# - Batched: Uses the DuckDB driver with a buffer, intended for local setups with medium throughput
#   - path: string -> path to duckdb file
#   - size: unsigned integer -> size of buffer (default: 100)
# - Clickhouse: Uses ClickHouse as a full DBMS for metric storage. Recommended for distributed setups.
#   - { todo!() }
database: !DuckDB
  path: satan.db

# Wether to insert metrics directly after ingesting or as a bulk insert after all tests are executed
delayed: false

# Configuration for ingest driver, the same tag handling applies here too
# - Exec: A script that takes the output of the solver as stdin and produces metrics to stdout
#   - timeout: unsigned integer -> timeout in ms for ingest script (default: 5000 ms)
#   - executable: string -> path to ingest executable
# - Null: The solver outputs the TestMetrics natively in YAML as stdout
ingest:
  cadical: !Exec
    timeout: 2000
    executable:  ./ingestors/cadical.py
  minisat: !Exec
    timeout: 2000
    executable:  ./ingestors/minisat.py

# Map of solvers <name>:<test set attrbutes>
solvers:
  cadical:
    # Path to binary for executing SAT solver
    # NOTE: will be executed with: <exec> <solver params> <test set params> <test file>
    exec: <path to solver bin>
    # Name of ingest module, see ingest.cadical above
    ingest: cadical
    # parameters that are applied after the solver params and before the test set params
    params: ""

# Map of test sets <name>:<test set attrbutes>
tests:
  cadical-tests:
    # Collectors: Components that retrieve and prepare the DIMACS test files
    # - Glob: Collect files from a directory by a glob (old default)
    # - GBD: Collect tests from a GBD compatible web host and save as local files (todo)
    # - Grouped: See below
    collector: !Glob
      # Glob for selecting files in path(s)
      glob: "*.cnf"
      # reference to directory (can be used with(out) paths)
      path: ./solvers/cadical/test/cnf
      # reference to directories to search
      paths:
        - ./solvers/cadical/test/cnf
        - ./solvers/cadical/test/cnf-2
  grouped-cadical-tests:
    # - Grouped: Union of collector tests (note: nested groups are not possible atm)
    collector: !Grouped
      collectors:
	- cadical-tests
    # unsigned integer -> timeout for test executions in ms
    # will overwrite solver timeout for this set of tests (optional)
    timeout: 10000
    # number of times each test is executed (default: 1)
    iterations: 10
    # params that are appended after solver params and before the test file
    params: ""
```

## Attribution

The (temporary) logo is `imp` from the [Firefox Emoji](https://github.com/mozilla/fxemoji) set, [licensed](https://github.com/mozilla/fxemoji/blob/gh-pages/LICENSE.md) under [CC BY 4.0](https://github.com/mozilla/fxemoji/blob/gh-pages/LICENSE.md#creative-commons-attribution-40-international-cc-by-40) by the Mozilla Foundation (Copyright 2015).

Some of the source code for the pooled connection managers was inspired by [ivanceras r2d2-sqlite crate](https://github.com/ivanceras/r2d2-sqlite).
There was no code re-used nor borrowed however it still deserves a shoutout for the good documentation.
