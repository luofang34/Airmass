# Weather feature boundary

## Purpose

Airmass emits immutable weather snapshots. A map uses geographic features.
`airmass-geojson` converts each applicable snapshot transition into typed
feature changes. The crate has no renderer dependency.

The adapter owns semantic mapping. A composition application owns the renderer
binding.

## Typed API

The adapter supplies this renderer-neutral vocabulary:

```rust,ignore
pub enum FeatureDelta {
    Upsert(WeatherFeature),
    Remove {
        id: WeatherFeatureId,
        reason: FeatureRemovalReason,
    },
}
```

An `Upsert` places or replaces one feature. A `Remove` deletes one feature. The
removal reason is `Expired` or `Replacement`.

`map_snapshot_transition` compares two snapshot envelopes. Pass `None` instead
of the first envelope to make an initial feature set. The function uses
`product_revision` to ignore a product that did not change. It keeps
`snapshot_revision`, `product_revision`, and `source_product_revision` as
different values in each upsert.

## Text report points

A typed text report becomes a point when these values are present:

- report type;
- station identity;
- station position.

The caller supplies the station position through `StationPositionResolver`.
This boundary lets the application use its navigation data. Airmass does not
depend on `Navdata`.

Use one stable resolver for one snapshot sequence. To apply a different station
data set, clear the destination feature set. Then map the current snapshot as
an initial feature set.

The feature identity contains the producer identity, product identity, report
type, station identity, and duplicate occurrence. A report replacement at the
same station keeps its feature identity. A producer replacement makes a new
feature identity.

`TextReportFeature` keeps the complete typed report. Thus, each field keeps its
time, quality, provenance, and absence reason. A METAR or SPECI feature keeps
its typed flight category field.

## Vector product rule

The adapter maps a value only when Airmass supplies typed vector data. It does
not decode an opaque payload. It does not infer hazard geometry from text.

A gridded product produces no feature and no error. A grid needs a raster data
path. It is not a GeoJSON feature.

## GeoJSON encoding

GeoJSON is an optional encoding. It is not the Rust API. The `geojson` feature
adds `encode_feature_delta`.

An upsert becomes an RFC 7946 Feature with a Point geometry. The coordinate
order is longitude and then latitude. A removal uses a null geometry. The
properties contain the operation, all three revision values, source selection,
and the complete typed report.

The default crate build does not link `serde_json`. The default build and the
`geojson` build are both `no_std + alloc`.

## Application boundary

The application combines weather and traffic features. It also owns display
policy and the renderer binding. The adapter does not define colors, symbols,
labels, grouping, filtering, or camera behavior.
