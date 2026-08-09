# Airmass roadmap

This document states the order of work. Each step gives its goal, its
acceptance, and the reason for its position in the order. No step is started.

A later step must not begin before its stated dependency is complete. The order
puts the input contract first, because every step after it consumes that
contract.

## Step 1. Take the input contract from aero-link

**Goal.** Read `FisBMessage` values from `aero-link` and prove that Airmass
receives what the contract promises.

`aero-link` supplies the FIS-B product projection through
`ReceptionEvent::fis_b_messages()`. That method returns nothing for a traffic
downlink. It is not a weather projection: it carries weather products,
aeronautical notices, and link-service reports together. A product router sends
each product to its owner, and Airmass accepts only the products that the
routing table assigns to it. Airmass must not call `ReceptionEvent::traffic()`.

Each message carries:

- the product identifier and its assignment status;
- the encoding family, such as Global Block, DLAC text, or text and graphic
  overlay;
- the product time;
- the segmentation fields, when the APDU is one part of a product file;
- the payload bytes, unchanged.

**Acceptance.**

- Airmass depends on `aero-link` with `default-features = false`.
- `ci/aero-link.ref` names the revision to build against.
- A test feeds a mixed stream and shows that traffic receptions produce no
  weather product and no error.
- A test shows that an unknown product keeps its payload bytes unchanged.

**Reason for the position.** Every later step reads this input. A change to the
input contract after step 3 would cause rework in each step.

## Step 2. Take complete products from the shared assembler

**Goal.** Accept a `CompleteFisBProduct` and keep only the products that the
routing table assigns to Airmass.

Segment assembly does not belong to Airmass. Pilotage ADR-0036 puts bounded
assembly in `aero-link`, because one uplink stream serves Airmass and Airspace
and two assemblers would repeat a mechanism and could disagree.

**Acceptance.**

- Airmass accepts a complete product and its product identifier.
- Airmass ignores a product that the routing table assigns to another owner.
- A test supplies a mixed product sequence and shows that Airmass keeps only
  meteorological products.

**Reason for the position.** A product decoder needs a complete payload, and
the routing rule decides which products arrive at all.

## Step 3. Hold the current set of products

**Goal.** Keep the product that is current, and remove the product that is not.

FIS-B repeats each product. A later transmission replaces an earlier one. Each
product also has a valid period. A product that is too old must not stay in the
current set.

**Acceptance.**

- A newer product replaces an older product with the same identity.
- An expired product leaves the current set.
- A caller supplies the time. Airmass contains no clock.
- A test shows that an out-of-order arrival does not replace a newer product
  with an older one.

**Reason for the position.** Lifecycle is the state that `aero-link` cannot
hold, so it is the first thing that only Airmass can supply. Product decoders
can arrive later and use this set.

## Step 4. Decode the text products

**Goal.** Convert DLAC text products into typed reports.

Product 413 carries METAR, SPECI, PIREP, TAF, amended TAF, and winds and
temperatures aloft. `aero-link` already decodes the DLAC characters. Airmass
gives each report its type, its location, and its time.

**Acceptance.**

- Airmass identifies the report type and the location for each report.
- Airmass keeps the report text unchanged.
- A test uses recorded reports from a live capture.

**Reason for the position.** Text products need no image decoder and no
palette. They give a usable output early and prove steps 2 and 3 with real
data.

## Step 5. Decode the Global Block products

**Goal.** Convert Global Block products into a grid that an application can
draw.

Eight products share the Global Block envelope: regional and CONUS
precipitation, icing at two altitude ranges, cloud tops, turbulence at two
altitude ranges, and lightning. The envelope is common. The values, the
altitude meaning, and the no-data value are not.

**Acceptance.**

- One decoder reads the common envelope: the block numbers, the empty-block
  map, and the block reference.
- A product-specific decoder reads the values for each product identifier.
- A test shows that an unsupported product identifier keeps its payload bytes
  and produces no grid.

**Reason for the position.** This step is the largest. It must follow assembly,
because a precipitation image is almost always segmented.

## Step 6. Decode the meteorological overlay products

**Goal.** Convert SIGMET, Convective SIGMET, AIRMET, G-AIRMET, and center
weather advisory products into typed records with their shapes.

NOTAM, TFR, and special-use airspace status are not in this step. `Airspace`
owns them. An earlier version of this roadmap put them here, which mixed two
categories in one component.

**Acceptance.**

- Airmass reads both the text records and the graphic overlay records.
- Airmass supplies the shape points for a graphic record.
- A test shows that Airmass rejects a notice product, because that product
  belongs to `Airspace`.

**Reason for the position.** These products need a complete payload from step 2
and the record structure from step 4. Their shapes need no palette, so they can
follow the Global Block work or run beside it.

## Step 7. Record and replay

**Goal.** Write received uplinks to a file, and read them back through the same
decoders.

**Acceptance.**

- A recording holds enough information to reproduce the product set.
- A replay of one recording gives the same product set every time.

**Reason for the position.** A recording is most useful after the decoders
exist, because the decoders state what the recording must keep. A flat file of
`ReceptionEvent` values already exists from live capture, so this step improves
an available method rather than creating the first one.

## Not in this roadmap

- Rendering, tiles, and palettes. An application owns them.
- Aircraft tracks. `Surveillance` owns them.
- NOTAM, TFR, and special-use airspace status. `Airspace` owns them.
- The NASR and CIFP baseline. `Navdata` owns it.
- Radio control, USB, framing, and FIS-B segment assembly. `aero-link` owns
  them.
- Stateless single-APDU decoding. `aero-link` owns it, per its issue #10.
