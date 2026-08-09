# Airmass

`Airmass` is a platform-independent Rust library for meteorological state. It
accepts weather data from local receivers, installed avionics, network
providers, replay, and simulation.

The Airmass core consumes a source-neutral `WeatherIngress` value. A
source-neutral value does not contain a source protocol type. The core keeps
weather revisions, validity, expiry, cancellation, and source comparison. It
publishes immutable weather snapshot handles.

The core does not depend on `aero-link` or `CompleteFisBProduct`. It does not
acquire radio samples, assemble FIS-B segments, classify protocol records, or
process traffic.

Meteorological data is perishable. It has no cycle-dated baseline. This rule
separates Airmass from `Navdata`. `AeronauticalUpdates` keeps notices,
cancellations, restrictions, and aeronautical operational status.

## Position in the system

```text
local receiver --> aero-link --> classified FIS-B records
                                      |
                                      v
                              airmass-aero-link
                                      |
network provider --> provider adapter +--> WeatherIngress
installed avionics -> source adapter  |
replay or simulator -> source adapter |
                                      v
                                Airmass core
                                      |
                                      v
                         immutable weather snapshots
```

The `airmass-aero-link` adapter consumes classified FIS-B protocol records. It
emits `WeatherIngress` values for weather records. It does not send notices or
link-health records to the Airmass core.

The adapter maps Product 413 data to typed text reports. Each report keeps its
type, station identity, report time, and source text. A METAR or SPECI report
also keeps its flight category when the ceiling and visibility are known.
Unsupported weather products keep their source payload and an explicit absent
typed value.

`aero-link` owns bounded FIS-B assembly and protocol classification. It uses
category names such as weather, aeronautical update, link health, and
unclassified. It does not select an application owner. Composition can send
one classified record to more than one consumer.

## Boundary

The Airmass core is synchronous and has one writer. The caller supplies civil
UTC and monotonic time. The core contains no clock, channel, task, lock, timer,
socket, USB stack, command-line interface, or renderer.

The core emits domain values. An application can show, record, or send these
values. A gridded weather product is a typed grid. It is not a renderer feature
or a tile.

The [weather ingress conformance corpus](conformance/README.md) supplies
source-neutral lifecycle scenarios. A source adapter can use the corpus before
a live source is available.

[AGENTS.md](AGENTS.md) states the binding repository rules. The
[roadmap](docs/roadmap.md) states the order of work.

## Related work

- [Airmass #2](https://github.com/luofang34/Airmass/issues/2) defines the
  source-neutral core contract.
- [Airmass #1](https://github.com/luofang34/Airmass/issues/1) defines the
  `airmass-aero-link` adapter.
- [Airmass #9](https://github.com/luofang34/Airmass/issues/9) defines typed
  DLAC weather reports.
- [aero-link #62](https://github.com/luofang34/aero-link/issues/62) defines the
  classified FIS-B record contract that the adapter requires.
- [Pilotage domain snapshot envelope](https://github.com/luofang34/Pilotage/blob/main/docs/domain-snapshot-envelope.md)
  defines the common immutable snapshot shape.
