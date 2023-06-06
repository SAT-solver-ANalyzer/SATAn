# SATAn debugging

SATAn features instrumentation with [`tracing`](https://tracing.rs/tracing/) that can provide useful information when debugging ingestors and solvers.
This includes some basic features like:

- runtime stats: Where does SATAn spend time? Could the MPI network have a high latency etc.
- debug output: information about data that is handled by SATAn, like ingestor output and solver output

All of this data can be either:

- inspected via the console: `RUST_LOG` environment variable (`debug`, `error`, `warn`,`info` and `trace`) ...
- or via an [OpenTelemetry](https://opentelemetry.io/)-compatible server, like [jeager](https://www.jaegertracing.io/), via the tracing config entry

## Working with Jeager

!!!info "Jeager"

    [Jeager](https://www.jaegertracing.io/) is an open source monitoring application that allows for collecting, inspecting and storing tracing data.
    This is achieved with [tracing's OpenTelemetry subscriber](https://github.com/tokio-rs/tracing-opentelemetry).

    You can start a local jeager collector locally with: `#` `podman run -d -p6831:6831/udp -p6832:6832/udp -p16686:16686 -p14268:14268 jaegertracing/all-in-one:latest`

    You can then visit `http://localhost:16686/` to inspect your new jeager instance.

To tell SATAn to report to this server you need to compile with the `metrics` feature (enabled by default) and use the `-t` option.
The configuration for the server is done via [environment varibles as outlined in the SDK specification](https://opentelemetry.io/docs/specs/otel/configuration/sdk-environment-variables/#general-sdk-configuration).
