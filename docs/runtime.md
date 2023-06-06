# Runtime behaviour

??? info "Read Me"

    I (Cobalt) am not experienced with distributed applications and as such try to thourougly document SATAn.
    The particular goal of documenting the program flow is less to help software consumers and more to help developers, like myself, understand how all components interact with each other to make extension and bug hunting easier.
    At the moment most of this 'documentation' is rather rough and on paper which won't be useful in the long run.
    Due to the nature of active development the documentation below is by design usually one step behind and may drift over time.

The program flow can be roughly split up into three parts:

- Initilization
- Preflight
- Collection
- Execution
- Tear down

## Initilization

The initilization should be the shortest part and only concerns itself with loading the application configuration and connecting to, if configured, an open telemetry server.
An important note here is that the initilization only checks the syntax of the config but provides no sematic checking.

## Preflight

The Preflight phase covers everything needed to get collection, execution and storage started.

This includes:

- Checking config semantically
- Initilization of collectors, including checking glob syntax and potentially referenced paths
- Initilization of storage adapters, i.e., estabilishing database connections and schemas
- Initilization of executor, this is not relevant for FS and local but will includ the leader election for the MPI collector

## Collection

All paths to tests are collected.
This may include downloading files when using the GBD collector (TBD).
This is done beforehand to avoid runtime conflicts and to make reasoning about the distributed code easier.

## Execution

All test sets are executed on solvers with the defined number of iterations.
This is already covered in the components sections and mostly consists of:

1. take path/solver from collector
2. distribute to a worker thread/ worker node
3. execute solver on path
4. timeout and/or ingest output
5. if not delayed, store with database adapter, otherwise store in memory

## Tear Down

The tear down phase mostly covers, if configured, delayed metrics insertion and closing connections.
