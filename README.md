# SAT solver ANalyzer

An attempt at building a toolbox for analyzing performance and runtime characteristics of SAT solvers.
This project is currently in its initial development stage.

In collaboration with and initiated by Benjamin Kaiser and Robert Clausecker.
Developed by Cobalt.

## Current state

- metrics database:
  - (wip, let's see how it goes) duckdb
  - (planned, most likely in clustered scenarios) clickhouse OR postgresql
  - (planned-feature): Merging, i.e., take multiple metric sets and compile into single database
- config:
  - YAML, able to express executors, sets of solvers and sets of test sets (see below)
- executors:
  - local parallel executor: Supervises locally spawned SAT solvers with a thread pool ([rayon](https://github.com/rayon-rs/rayon) based, configurable concurrency, supports thread pinning)
    - The executor is only parallelizes the actual execution of the tests. This means that the initial process of finding the tests and preparing the data for the solvers may be bound by a single thread. This may be changed in the future but is sufficient for the current test suites.
- ingestors:
  - made simpel
  - `raw`: A simple ingestor that will call a specified executable with the output of a SAT solver run and expext the extracted metrics in a KV-format
- tests:
  - tests are grouped in tests sets and identified as files via a [glob](https://github.com/BurntSushi/ripgrep/tree/master/crates/globset) that may be searched within path(s) with [ignore](https://github.com/BurntSushi/ripgrep/tree/master/crates/ignore).
- ingest:
  - (WIP) minisat and cadical are planned as a first step

## Runtime behaviour (WIP)

### Runner

1. Read & Deserialize config
2. Check for consistency etc., so called pre-flight checks
3. Construct Tasks, i.e., figure out what tests exists and map them to solvers
4. Start executing 
5. (if neccessary), teardown of stateful components like database connections

## Building

### Runner

Required dependencies:

- A rust toolchain (MSRV: `stable` - 1.68.2), recommeded: [rustup](https://rustup.rs/)
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
# - Slurm { todo!() }
executor: !Local
  pinned: true

# Configuration for metric storage driver, the same tag handling applies here too
# - DuckDB: Uses DuckDB, an sqlite-like file based database, for storage. Recommended for local setups.
#           This driver is, like SQLite, limited to one write at time and works with an internal Mutex.
#   - path: string -> path to duckdb file
# - Batched: Uses the DuckDB driver with a buffer, intended for local setups with medium throughput
#   - path: string -> path to duckdb file
#   - size: unsigned integer -> size of buffer (default: 100)
# - Clickhouse: Uses ClickHouse as a full DBMS for metric storage. Recommended for distributed setups.
#   - { todo!() }
database: !DuckDB
  path: satan.db

# Configuration for ingest driver, the same tag handling applies here too
# - Exec: A script that takes the output of the solver as stdin and produces metrics to stdout
#   - timeout: unsigned integer -> timeout in ms for ingest script (default: 5000 ms)
#   - executable: string -> path to ingest executable 
ingest:
  cadical: !Exec
    timeout: 2000
    executable:  ./solvers/cadical.py
  minisat: !Exec
    timeout: 2000
    executable:  ./solvers/minisat.py

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
    # reference to directory (can be used with(out) paths) 
    path: ./solvers/cadical/test/cnf
    # reference to directories to search
    paths:
      - ./solvers/cadical/test/cnf
      - ./solvers/cadical/test/cnf-2
    # unsigned integer -> timeout for test executions in ms
    # will overwrite solver timeout for this set of tests (optional)
    timeout: 10000 
    # number of times each test is executed (default: 1)  
    iterations: 10
    # Glob for selecting files in path(s)
    glob: "*.cnf"
    # params that are appended after solver params and before the test file
    params: ""
```



## Napkin architecture drawing

> Open for changes etc. at any time.
> NOTE:
> - The store for metrics may be one of DuckDB, Clickhouse or maybe something else that fits the data structure.
> - The executor is part of the runner and only one executor maybe active for one runner.

<center>

![](https://nextcloud.cobalt.rocks/s/DFywjjrLXb4kj5x/download/Untitled-2023-04-18-2045%281%29.png)

</center>


## Logo attribution

The (temporary) logo is `imp` from the [Firefox Emoji](https://github.com/mozilla/fxemoji) set, [licensed](https://github.com/mozilla/fxemoji/blob/gh-pages/LICENSE.md) under [CC BY 4.0](https://github.com/mozilla/fxemoji/blob/gh-pages/LICENSE.md#creative-commons-attribution-40-international-cc-by-40) by the Mozilla Foundation (Copyright 2015).
