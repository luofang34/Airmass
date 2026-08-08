# Airmass

`Airmass` is a platform-independent Rust library that converts FIS-B ground
uplinks into current weather products. It accepts decoded FIS-B application
protocol data units from `aero-link`. It emits weather products that an
application can show, record, or send to another system.

The library owns the state that spans more than one reception: assembly of a
product file from its segments, replacement of a product when a newer one
arrives, expiration of a product that is too old, and per-product history. It
never acquires radio samples, never owns aircraft tracks, and never contains a
runtime, socket, or renderer.

FIS-B carries weather and aeronautical information on 978 MHz. A ground station
transmits it. An aircraft receives it. The information includes precipitation
images, cloud tops, turbulence, icing, lightning, NOTAMs, temporary flight
restrictions, SIGMETs, and text reports such as METAR and TAF.

[AGENTS.md](AGENTS.md) states the binding rules. The
[roadmap](docs/roadmap.md) states the order of work and the acceptance for each
step. Airmass has no implementation yet.

## Position in the system

```text
978 MHz radio
      |
      v
aero-link            framing, FEC, one-APDU FIS-B decode
      |
      +---- traffic ------> Surveillance      aircraft tracks
      |
      +---- FIS-B --------> Airmass           weather products
```

`aero-link` decodes one APDU at a time and keeps no state between receptions.
It supplies the product identifier, the product time, the segmentation fields,
and the payload bytes. `Airmass` performs every operation that needs more than
one APDU or more than one reception.

`Surveillance` and `Airmass` are siblings. Neither reads the other. A reception
that carries traffic goes to `Surveillance`. A reception that carries a ground
uplink goes to `Airmass`. `aero-link` makes the split: `traffic()` returns
nothing for an uplink, and `fis_b_messages()` returns nothing for a downlink.

## Boundary

Airmass starts after FIS-B APDU decoding. It contains no USB stack, no radio
control, no aircraft track state, and no map renderer.

An application composes the outputs. It can put a weather layer and a traffic
layer on one map. Airmass does not draw the layer, and Surveillance does not
supply the weather.

## Status

The repository holds the design and the roadmap. Work starts at step 1 of the
[roadmap](docs/roadmap.md).

Related work:

- `aero-link` issue #10 records the product-decode ownership split.
- `aero-link` issue #34 records the decision to put the weather consumer here.
- `aero-link` issue #33 identified the products that a ground station sends.
