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

Airmass converts FIS-B ground uplinks into current weather products. It accepts
`FisBMessage` values from `aero-link`. It emits weather products that an
application can show, record, or send to another system.

Airmass owns the state that spans more than one reception. `aero-link` decodes
one APDU at a time and keeps no state. Airmass assembles product files from
segments, replaces a product when a newer one arrives, and removes a product
when it expires.

## Hard boundary

Airmass starts after FIS-B APDU decoding. It never acquires radio samples and
never processes traffic.

Airmass must not depend on or contain:

- `nusb`, libusb, serial discovery, USB descriptors, FTDI, CDC, RTL-SDR, tuner
  control, or any USB stack;
- DriverKit, Swift, Objective-C, or Xcode types;
- aircraft tracks, track identity, CPR position reconstruction, or fusion.
  Those belong to `Surveillance`.

Airmass reads only the weather projection of a reception. `aero-link` supplies
it through `ReceptionEvent::fis_b_messages()`. Airmass must not read
`ReceptionEvent::traffic()`.

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

- `aero-link` decodes one APDU without state. It supplies the product
  identifier, the product time, the segmentation fields, and the payload bytes.
- Airmass performs every operation that needs more than one APDU or more than
  one reception.

Do not add weather state to `aero-link`. Do not add FIS-B decoding to
`Surveillance`.
