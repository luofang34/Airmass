# Weather ingress conformance corpus

`weather-ingress-v3.json` defines source-neutral weather lifecycle scenarios.
The corpus version is independent of a source protocol. The file also names
the `WeatherIngress` schema version that its steps use.

The evidence profile states the common origin, delivery path, quality, time
basis, time quality, time uncertainty, and raw product identity. A missing
source time is explicitly unknown.

The corpus gives monotonic step values in milliseconds. The exported schema
stores monotonic values and their uncertainty in microseconds.

Each scenario starts with an empty `WeatherStore`. A `product` or
`cancellation` step supplies one source event. An `advance` step supplies time
without source input.

An adapter can use each source step as its expected output. The adapter must
make an equivalent `WeatherIngress` value. It does not need a live receiver or
provider to use this corpus.

The Airmass test suite runs all scenarios against the public core API. The
suite checks source revision status, publication state, product revision,
snapshot revision, selected source, source epoch, selection reason, and next
deadline.
