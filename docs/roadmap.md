# Airmass roadmap

This document states the order of work. Each step gives its goal, acceptance,
and dependency.

The Airmass core uses source-neutral domain types. A source-neutral type does
not contain a source protocol type. A source adapter converts one source
contract to the domain contract.

## Product delivery sequence

The product delivery sequence has one dependency chain:

1. [Airmass #1](https://github.com/luofang34/Airmass/issues/1) adds the
   `airmass-aero-link` adapter.
2. [Airmass #9](https://github.com/luofang34/Airmass/issues/9) adds DLAC text
   reports and the METAR flight category.
3. [Airmass #7](https://github.com/luofang34/Airmass/issues/7) adds the
   renderer-neutral feature adapter.
4. [Airmass #13](https://github.com/luofang34/Airmass/issues/13) adds typed
   weather advisory shapes and their altitude bands.

Other product work has no dependency on this chain. Assign additional project
capacity to this chain first.

## Step 1. Define the source-neutral core

**Issue.** [Airmass #2](https://github.com/luofang34/Airmass/issues/2)

**Goal.** Define `WeatherIngress`, the synchronous weather lifecycle store,
and immutable weather snapshots.

The core accepts weather data from a local receiver, installed avionics, a
network provider, replay, or simulation through the same ingress contract. It
does not depend on `aero-link`, a provider adapter, or
`CompleteFisBProduct`.

Civil UTC controls issue, effective, and valid periods. Host-supplied monotonic
time controls receive order, age, and timeout. The core returns its next civil
UTC and monotonic deadlines to the host.

The store keeps a product revision for each weather product. It keeps a
snapshot revision for each published state. The implementation names the two
counters `product_revision` and `snapshot_revision`. It does not use one
counter for both purposes.

The snapshot follows the
[Pilotage domain snapshot envelope](https://github.com/luofang34/Pilotage/blob/main/docs/domain-snapshot-envelope.md).
It carries a producer instance ID, a snapshot revision, and an immutable
weather snapshot. A caller can retain a snapshot handle after the store
changes.

Each source-derived field uses the `TimedField<T>` pattern. This pattern keeps
the value, time evidence, quality evidence, and provenance together. An absent
field carries a typed absence reason. A versioned exported schema lets a reader
reject an unsupported version before it reads the payload.

**Acceptance.**

- The ingress and snapshot schemas are versioned.
- The core is synchronous and has one writer.
- The caller supplies all time values.
- The core returns its next deadline.
- Product revision and snapshot revision use distinct types and counters.
- Each counter uses `wrapping_add(1)` and rejects a wrapped value.
- Source comparison is deterministic and supplies an explanation.
- A retained snapshot handle does not change when the store changes.
- A conformance corpus covers revision, cancellation, expiry, conflicting
  sources, unknown source time, replay, and source restart.
- The core builds for each selected portable target.
- A dependency check rejects source-adapter and link-service dependencies from
  the core.

**Dependency.** The core contract has no source dependency.

## Step 2. Add the aero-link source adapter

**Issue.** [Airmass #1](https://github.com/luofang34/Airmass/issues/1)

**Goal.** Add an `airmass-aero-link` adapter that maps classified FIS-B
protocol records to `WeatherIngress`.

`aero-link` owns bounded product assembly and protocol classification. The
adapter preserves source identity, source epoch, delivery path, monotonic
ingress time, source time, time quality, uncertainty, and raw product identity.
It does not apply weather lifecycle rules.

The adapter emits no weather ingress for a notice or a link-health record.
TWGO uses one protocol category for more than one domain. The adapter admits
weather advisory product identifiers from that category. Composition can send
one classified record to more than one consumer.

The adapter emits no weather ingress for a Global Block product. A typed grid
decoder is not available. The adapter counts each completed Global Block
product that it ignores.

**Acceptance.**

- The adapter depends on `aero-link` with default features disabled.
- `ci/aero-link.ref` names the exact revision used by CI.
- A fixture maps each decoded weather product to `WeatherIngress`.
- A mixed Product 8 fixture emits no weather ingress for a notice or a
  link-health record.
- A Global Block product produces no weather ingress.
- A reception with a Global Block product still emits its other weather
  products.
- A public counter reports each completed Global Block product that the adapter
  ignores.
- Provenance survives the mapping without a default value.
- A source restart creates a new source epoch.
- A traffic downlink produces no FIS-B weather message and no error.
- An unknown product keeps its payload bytes unchanged.
- No code calls `ReceptionEvent::traffic()`.
- `cargo tree` shows no `aero-link` dependency from the Airmass core.

**Dependency.** This step starts only after Step 1 and
[aero-link #62](https://github.com/luofang34/aero-link/issues/62) are complete.

## Step 3. Add typed weather values

**Goal.** Convert admitted weather evidence into typed reports, hazards, and
grids.

The adapter keeps the protocol classification that `aero-link` supplies.
Airmass adds only weather-domain meaning. It does not repeat protocol fields
that `aero-link` already decodes.

A gridded product is a typed grid. It is not a feature, tile, image, or renderer
value.

**Acceptance.**

- Text reports keep their report type, station identity, report time, and
  text.
- Hazards keep their validity and typed geometry when geometry exists.
- Advisory altitude bands keep their source base and top.
- A forecast-point advisory keeps its point validity.
- A single source altitude stays separate from an altitude band.
- An advisory without a source band keeps an absent band field.
- Gridded products use a typed grid with explicit coordinates and no-data
  values.
- Each decoded field keeps its own time, quality, and provenance.
- An unsupported weather value stays available as source evidence.

**Dependency.** This step requires the core contract and one source adapter.

## Step 4. Record and replay

**Goal.** Record versioned ingress values and replay them through the same
core contract.

**Acceptance.**

- A recording keeps the ingress schema version and all provenance.
- Replay does not replace source time with replay time.
- A source epoch separates two replay sessions.
- The same recording produces the same published weather state.

**Dependency.** This step requires the versioned ingress schema from Step 1.

## Not in this roadmap

- Application-owner routing. Composition selects installed consumers.
- Radio control, USB, framing, FIS-B assembly, and protocol classification.
  `aero-link` owns them.
- Aircraft tracks. `Surveillance` owns them.
- Notices, TFRs, restrictions, cancellations, and aeronautical operational
  status. `AeronauticalUpdates` owns them.
- The NASR and CIFP baseline. `Navdata` owns it.
- Rendering, features, tiles, images, and presentation schemas. An application
  owns them.
