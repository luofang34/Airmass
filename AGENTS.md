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

Do not copy noncompliant legacy text into a new document. Record a broad legacy
rewrite as separate work.

## Primary goal

Airmass holds meteorological state. It accepts weather products from more than
one source. It emits weather products that an application can show, record, or
send to another system.

Airmass owns the state that spans more than one reception. A source supplies one
product at a time. Airmass keeps the current product, replaces a product when a
newer one arrives, and removes a product when it expires.

Meteorological data is perishable. It has no cycle-dated baseline. That is the
property that separates Airmass from `Navdata`.

## Hard boundary

Airmass holds meteorological state only. It never acquires radio samples and
never processes traffic.

Airmass must not depend on or contain:

- `nusb`, libusb, serial discovery, USB descriptors, FTDI, CDC, RTL-SDR, tuner
  control, or any USB stack;
- DriverKit, Swift, Objective-C, or Xcode types;
- aircraft tracks, track identity, CPR position reconstruction, or fusion.
  Those belong to `Surveillance`;
- NOTAM, TFR, or special-use airspace state. Those belong to `Airspace`;
- the NASR or CIFP baseline. That belongs to `Navdata`;
- a map renderer or a tile format.

A 978 MHz ground uplink is not a weather stream. It carries weather products,
aeronautical notices, and link-service reports together. A product router sends
each product to its owner. Airmass accepts only the products that the routing
table assigns to it.

Airmass must not read `ReceptionEvent::traffic()`.

## Execution rules

Pilotage ADR-0036 states the execution model. Airmass follows it.

- The Airmass store is a synchronous, single-writer core.
- The caller supplies the time. Airmass contains no clock.
- Airmass contains no channel, no task, no lock, and no timer.
- An asynchronous provider trait belongs in an adapter package, not in the
  store.
- Each channel between Airmass and another component states its bound, its
  backpressure policy, the owner of a dropped item, and the drop counter.

## Dependency on aero-link

Depend on the portable API only:

```toml
aero-link = { path = "../aero-link", default-features = false }
```

An application can enable `aero-link/std`. Airmass core must not require
`aero-link/host` or an Apple or board adapter.

CI reads `ci/aero-link.ref`. That file names the `aero-link` revision to build
against. `Surveillance` uses the same method.

## Product decode ownership

`aero-link` issue #10 records the split. Keep to it.

- `aero-link` decodes one APDU without state. It assembles the segments of one
  product file. It supplies the product identifier, the product status, the
  encoding family, the product time, and the payload bytes.
- Airmass performs every operation that needs more than one product or more
  than one source.

Do not add weather state to `aero-link`. Do not add a second segment assembler.
