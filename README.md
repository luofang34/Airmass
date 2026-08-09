# Airmass

`Airmass` is a platform-independent Rust library that holds meteorological
state. It accepts weather products from more than one source: FIS-B ground
uplinks through `aero-link`, and network providers such as the Aviation Weather
Center. It emits weather products that an application can show, record, or send
to another system.

The library owns the state that spans more than one reception: replacement of a
product when a newer one arrives, expiration of a product that is too old,
comparison between two sources of the same product, and per-product history. It
never acquires radio samples, never assembles FIS-B segments, never owns
aircraft tracks or notices, and never contains a runtime, socket, or renderer.

Meteorological data is perishable and has no cycle-dated baseline. That property
separates Airmass from `Navdata`, which holds a baseline, and from `Airspace`,
which holds notices that amend a baseline.

FIS-B carries weather and aeronautical information on 978 MHz. A ground station
transmits it. An aircraft receives it. Airmass takes only the meteorological
part: precipitation images, cloud tops, turbulence, icing, lightning, SIGMET,
AIRMET, and text reports such as METAR and TAF. NOTAM, temporary flight
restrictions, and special-use airspace status go to `Airspace`.

[AGENTS.md](AGENTS.md) states the binding rules. The
[roadmap](docs/roadmap.md) states the order of work and the acceptance for each
step. Airmass has no implementation yet.

## Position in the system

```text
978 MHz radio                      network providers
      |                                   |
      v                                   |
aero-link      framing, FEC, decode,      |
               bounded segment assembly   |
      |                                   |
      +---- traffic ---------> Surveillance
      |
      +---- complete FIS-B product --> product router
                                          |
                    weather products -----+----> Airmass <--- AWC and others
                    notices --------------+----> Airspace
                    link-service ---------+----> diagnostics
```

`aero-link` decodes one APDU at a time and assembles the segments of one product
file. It supplies the product identifier, the product status, the encoding
family, the product time, and the payload bytes. One assembler serves every
category, because one uplink stream carries all of them.

`Surveillance`, `Airmass`, and `Airspace` are siblings. None reads another. A
reception that carries traffic goes to `Surveillance`. A complete FIS-B product
goes to the owner that the routing table names.

## Boundary

Airmass holds meteorological state only. It contains no USB stack, no radio
control, no segment assembler, no aircraft track state, no notice state, no
navigation baseline, and no map renderer.

The Airmass store is a synchronous, single-writer core. The caller supplies the
time. Asynchronous execution belongs between components, per Pilotage ADR-0036.

An application composes the outputs. It can put a weather layer and a traffic
layer on one map. Airmass does not draw the layer, and Surveillance does not
supply the weather.

## Status

The repository holds the design and the roadmap. Work starts at step 1 of the
[roadmap](docs/roadmap.md).

Related work:

- Pilotage ADR-0036 assigns domain state by data category and states the
  execution and channel rules.
- `aero-link` issue #10 records the product-decode ownership split.
- `aero-link` issue #34 records the decision to put the weather consumer here.
- `aero-link` issue #33 identified the products that a ground station sends and
  supplies the routing table.
