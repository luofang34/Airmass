# UAT test data

`uat/uplink-captured-corrected.hex` is one corrected 978 MHz ground uplink from
a live capture. It contains one Product 413 winds and temperatures aloft
report.

The `advisories` directory contains weather advisory source values from the
Aviation Weather Center API. Each `twgo_payload_hex` field encodes those values
with the FAA TWGO format. The fixed bytes make each test repeatable. They are
not radio captures.

`independent-golden-g-airmet.json` contains exact TWGO bytes from the
`fisb-decode` TG30 golden vector. The fixture names the source commit, file, and
line. The test checks the result against the independent decoder output.

`recorded-convective-sigmet-pair.json` contains one Product 12 graphical APDU
and its matching text APDU. The `fisb-decode` TG30 recording supplies both
APDUs. The text APDU identifies the advisory as a Convective SIGMET.

`recorded-airmet-graphical.json` contains one Product 11 graphical APDU and one
Product 11 text APDU from the same recording. It verifies the Product 11 flag
values and text grammar.

`faa-cwa-text.json` contains a Product 15 text example from the FAA. The test
encodes the text in one TWGO text record.
