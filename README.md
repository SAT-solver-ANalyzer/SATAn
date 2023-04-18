# SAT solver ANalyser

An attempt at building a toolbox for analysing performnce and runtime charasterics of SAT solvers.
This project is currently in its initial development stage.

In collaboration with and initiated by Benjamin Kaiser and Robert Clausecker.
Developed by Cobalt.

## Current state

- config:
  - YAML, able to express executors, sets of solvers and sets of test sets (see below)
- executors:
  - local parallel executor: Supervises locally spawned SAT solvers with a thread pool ([rayon](https://github.com/rayon-rs/rayon) based, configurable concurrency)
- tests:
  - tests are grouped in tests sets and identified as files via a [glob](https://github.com/BurntSushi/ripgrep/tree/master/crates/globset) that may be searched within path(s) with [ignore](https://github.com/BurntSushi/ripgrep/tree/master/crates/ignore).
- ingest:
  - (WIP) minisat

## Napkin architecture drawing

> Open for changes etc. at any time.
> NOTE:
> - The datastore for metrics may be one of DuckDB, (maybe) Clickhouse or maybe something else that fits the data structure.
> - The executor is part of the runner and only one executor maybe active for one runner.

<center>

![Diagram](https://outline.cobalt.rocks/api/attachments.redirect?id=4f654f9b-91de-4f28-afab-939e5b92d6fb)

</center>


## Logo attribution

The (temporary) logo is `imp` from the [Firefox Emoji](https://github.com/mozilla/fxemoji) set, [licensed](https://github.com/mozilla/fxemoji/blob/gh-pages/LICENSE.md) under [CC BY 4.0](https://github.com/mozilla/fxemoji/blob/gh-pages/LICENSE.md#creative-commons-attribution-40-international-cc-by-40) by the Mozilla Foundation (Copyright 2015).
