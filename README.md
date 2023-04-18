# SAT solver ANalyser

An attempt at building a toolbox for analysing performnce and runtime charasterics of SAT solvers.
This project is currently in its initial development stage.

In collaboration with and initiated by Benjamin Kaiser and Robert Clausecker.
Developed by Cobalt.

## Current state

- config:
  - YAML, able to express executors, sets of solvers and sets of test sets (see below)
- executors:
  - local parallel executor: Supervises locally spawned SAT solvers with a thread pool ([rayon](https://github.com/rayon-rs/rayon) based, configurable concurrency, supports thread pinning)
- tests:
  - tests are grouped in tests sets and identified as files via a [glob](https://github.com/BurntSushi/ripgrep/tree/master/crates/globset) that may be searched within path(s) with [ignore](https://github.com/BurntSushi/ripgrep/tree/master/crates/ignore).
- ingest:
  - (WIP) minisat and cadical are planned as a first step

## Building

> TODO: Document how to use nix with the nix devshell

Required:

- A rust toolchain (MSRV: `stable` - 1.68.2), recommeded: [rustup](https://rustup.rs/)

Building:

- To create a debug build (placed in `target/debug/satan-runner`): `cargo b`
- To create a release, i.e. optimized, build (placed in `target/release/satan-runner`): `cargo b --release`

## Config

> TODO: Document all options

```yaml
executor:
  # Name of used executor, only "local" is supported for now
  name: local
  parameter:
    # This can be either:
    # a number specifying the number of threads in the thread pool (default: number of logical CPUS)
    # "pinned": -> number of logical CPUs but pinned with sched_setaffinity (linux only)
    threads: pinned

# Map of tests <name>:<test set attrbutes>
tests:
  cadical-tests:
    # references git submodule for cadical, may be a relative or absolute path
    path: ./solvers/cadical/test/cnf
    # timeout for cadical executions in nano seconds (10000000000 ns = 10 s)
    timeout: 10000000000
    # Glob for selecting files in path, will match all files ending with .cnf
    glob: "*.cnf"
    # TODO: Document paths and params

solvers:
  cadical:
    # Path to binary for executing SAT solver
    # NOTE: will be executed with: <exec> <solver params> <test set params> <test file>
    exec: <path to cadical bin>
    # Name of ingest module (not implemented yet)
    ingest: cadical
    # TODO: Document params
```

## Napkin architecture drawing

> Open for changes etc. at any time.
> NOTE:
> - The datastore for metrics may be one of DuckDB, (maybe) Clickhouse or maybe something else that fits the data structure.
> - The executor is part of the runner and only one executor maybe active for one runner.

<center>

![](https://nextcloud.cobalt.rocks/s/DFywjjrLXb4kj5x/download/Untitled-2023-04-18-2045%281%29.png)

</center>


## Logo attribution

The (temporary) logo is `imp` from the [Firefox Emoji](https://github.com/mozilla/fxemoji) set, [licensed](https://github.com/mozilla/fxemoji/blob/gh-pages/LICENSE.md) under [CC BY 4.0](https://github.com/mozilla/fxemoji/blob/gh-pages/LICENSE.md#creative-commons-attribution-40-international-cc-by-40) by the Mozilla Foundation (Copyright 2015).
