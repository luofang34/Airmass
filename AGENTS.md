# Airmass Repository Instructions

## Documentation language

All project-authored documentation MUST use ASD-STE100 Simplified Technical
English.

- Write in English.
- Use an approved word when it has the required meaning.
- Use one word for one meaning.
- Use short, direct sentences.
- Use active voice when you can identify the actor.
- Give one instruction or one main statement in each sentence.
- Define a necessary technical name before you use it.
- Use the same technical name in all related documents and diagrams.
- Keep code identifiers, protocol fields, quotations, and product names exact.
- Apply these rules to all new text and to all text that you change.

Do not copy noncompliant text into a new document. Record a broad rewrite as
separate work.

## Primary goal

Airmass holds meteorological state. It accepts weather data from more than one
source. It publishes weather data that an application can show, record, or
send to another system.

A source adapter emits one domain-owned `WeatherIngress` value at a time. The
Airmass core keeps the current state. It replaces state when it accepts a newer
revision. It removes data when validity or freshness ends.

Meteorological data is perishable. It has no cycle-dated baseline. This rule
separates Airmass from `Navdata`.

## Hard boundary

The Airmass core holds meteorological state only. It never acquires radio
samples and never processes traffic.

The core must not depend on or contain:

- `aero-link`, `CompleteFisBProduct`, or another source protocol type;
- a provider adapter or `airmass-aero-link`;
- `nusb`, libusb, serial discovery, USB descriptors, FTDI, CDC, RTL-SDR, tuner
  control, or another USB stack;
- DriverKit, Swift, Objective-C, or Xcode types;
- aircraft tracks, track identity, CPR position reconstruction, or fusion;
  `Surveillance` owns them;
- NOTAM, TFR, restrictions, cancellations, or special-use aeronautical status;
  `AeronauticalUpdates` owns them;
- the NASR or CIFP baseline; `Navdata` owns it;
- a map renderer, renderer feature, tile, or presentation schema.

A gridded weather product is a typed grid. It is not a renderer feature.

## Package boundaries

The Airmass core consumes `WeatherIngress`. This value is source-neutral. It
keeps source identity, source epoch, delivery path, time evidence, quality,
and provenance without a source protocol type.

The `airmass-aero-link` adapter can depend on `aero-link` and the Airmass core.
It consumes classified FIS-B protocol records and emits `WeatherIngress`.

The adapter must not exist before
[aero-link #62](https://github.com/luofang34/aero-link/issues/62) supplies its
classified-record contract. The adapter must not use an application-owner
routing table.

No Airmass package can call `ReceptionEvent::traffic()`.

## Execution rules

Pilotage ADR-0036 states the execution model. Airmass follows it.

- The Airmass store is a synchronous, single-writer core.
- The caller supplies civil UTC and monotonic time. Airmass contains no clock.
- The core contains no channel, task, lock, timer, socket, or executor.
- The core returns its next required deadline to the host.
- An asynchronous provider trait belongs in an adapter package.
- Each deployed channel states its bound, backpressure policy, drop owner, and
  drop counter.

## Dependency on aero-link

The repository workspace can declare this portable dependency. Only
`airmass-aero-link` can select it.

```toml
aero-link = { path = "../aero-link", default-features = false }
```

An application can enable `aero-link/std`. The adapter must not require
`aero-link/host` or a platform adapter.

CI reads `ci/aero-link.ref` for adapter checks. That file names one full
`aero-link` commit SHA. Core-only checks do not require the dependency.

## FIS-B ownership

[aero-link #62](https://github.com/luofang34/aero-link/issues/62) defines the
source-neutral FIS-B contract.

- `aero-link` owns bounded segment assembly and protocol classification.
- `aero-link` emits one classified protocol record for each Product 8 record.
- A classification uses protocol categories. It does not name an application
  owner.
- Composition can send one classified record to more than one consumer.
- `airmass-aero-link` maps weather records to `WeatherIngress`.
- Airmass owns weather revision, validity, cancellation, expiry, and source
  comparison.

Do not add weather state to `aero-link`. Do not add a FIS-B assembler to
Airmass.

## Related issues

- [Airmass #2](https://github.com/luofang34/Airmass/issues/2) defines the core.
- [Airmass #1](https://github.com/luofang34/Airmass/issues/1) defines the
  adapter.
