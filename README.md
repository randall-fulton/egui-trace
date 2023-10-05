# egui-trace

_Actively looking for a better name._

Local OpenTelemetry trace explorer app. Supports importing traces from files or starting an OpenTelemetry collector endpoint to collect data over HTTP.

## Installation

I haven't had the time to setup an actual release process, so you'll have to build it yourself.

1. Install latest Rust toolchain via [Rustup]().
1. Install [protoc]() (used to generate the OTel-compatible collector).

```
git clone git@github.com:randall-fulton/egui-trace.git
cd egui-trace
cargo install --path=.
```
