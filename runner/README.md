> WARNING: This is a project under heavy development and only intended for testing purposes at the moment.
> Please only use it, if you know what you are doing/ running on your system.

# SATAn runner

The SATAn runner is responsible for scheduling, executing and ingesting job metrics.

## Config

`todo!()`

## Architecture

> tldr;
> The runner's main job is composing *tasks* from *solvers* and *tests*.
> Each task is the *single* execution of a *solver* on a *test* handled by an *executor*.
> jobs are generated from the config by composing both solver parameters and jobset parameters into a group of jobs.

The purpose of this tool is to:

- schedule: manage the execution a group of benchmarks/ tests in an environment
- execute: start and, if a timeout is reached, terminate a job,
- collect job metrics: Record satisfiability results, parse logs and ingest into a data store

This is achived with a classic task queue at the moment.
Each task describes a single run of a SAT solver against a test file with a given set of parameters.
The runner will handle multiple solvers and test collections at the same time and iterate over all possible permutations.
However only a single executor may be used per SATAn runner.

With this each SAT is executed against a (sub)set of tests.
An execution consists of two sets of parameters, the test file and parameter set and the solver executable.
The effective parameters of a task are: `./SAT [sat parameters] [test set parameters] [test file]`, although both `sat parameters` and `test set parameters` are optional.

This execution is handled by the *executor* who is responsible for actually running the SAT locally or on remote system(s).

> NOTE: Only local execution with a thread pool that spawns the SAT is supported at the moment.
> Support for slurm is planned however there will quite some work involved to get this working.

> NOTE: The long term goal is to allow for some kind of customization regarding the logging ingest.
> The short term implementation should allow a user to specify regex(s) that will be applied against the output.
> The long term implementation would most likely feature an intermediate language, like lua, that could be embedded within the runner.
