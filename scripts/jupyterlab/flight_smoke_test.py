#!/usr/bin/env python3
"""tpt-soma Arrow Flight smoke test (JupyterLab / Python client).

Validates the pyarrow.flight client path against the tpt-soma Flight service
(TM-02: a capability token is required as an `authorization` header).

Prereqs:
  pip install pyarrow>=15
  # issue a token with the admin binary:
  #   cargo run -p tpt-soma-api --bin admin -- gen-key
  #   cargo run -p tpt-soma-api --bin admin -- issue \
  #       --subject researcher-1 --resource-class genomic_variant \
  #       --action read --cohort '*' --key dev-keys/signing_key.bin
  export TPT_FLIGHT_URL=grpc://localhost:8815
  export TPT_TOKEN='{...json token...}'

Usage:
  python flight_smoke_test.py [--flight-url URL] [--token JSON] [--data-type variants]
"""

import argparse
import json
import os
import sys

import pyarrow as pa
import pyarrow.flight as flight


def make_options(token: str) -> flight.FlightCallOptions:
    """Attach the capability token as an Authorization header."""
    return flight.FlightCallOptions(
        headers=[(b"authorization", ("Bearer " + token).encode("utf-8"))]
    )


def fetch_batches(client: flight.FlightClient, command: str, options):
    """Run get_flight_info + do_get and return a list of RecordBatches."""
    descriptor = flight.FlightDescriptor.for_command(command.encode("utf-8"))
    info = client.get_flight_info(descriptor, options=options)
    if not info.endpoints:
        raise RuntimeError("no endpoints returned")
    ticket = info.endpoints[0].ticket

    reader = client.do_get(ticket, options=options)
    batches = []
    while True:
        try:
            batch = reader.read_chunk().data
        except StopIteration:
            break
        batches.append(batch)
    return batches


def main() -> int:
    parser = argparse.ArgumentParser(description="tpt-soma Flight smoke test")
    parser.add_argument(
        "--flight-url",
        default=os.environ.get("TPT_FLIGHT_URL", "grpc://localhost:8815"),
    )
    parser.add_argument(
        "--token", default=os.environ.get("TPT_TOKEN", ""), help="capability token JSON"
    )
    parser.add_argument(
        "--token-file",
        default=os.environ.get("TPT_TOKEN_FILE", ""),
        help="path to file containing the capability token JSON",
    )
    parser.add_argument(
        "--data-type",
        default="variants",
        help="one of: variants, expression, umap, clinical_observations, cgm",
    )
    parser.add_argument(
        "--sample-id", default="00000000-0000-0000-0000-000000000000"
    )
    args = parser.parse_args()

    if args.token_file:
        with open(args.token_file, "r", encoding="utf-8") as fh:
            args.token = fh.read().strip()
    if not args.token:
        print("ERROR: a capability token is required (TPT_TOKEN or --token).")
        return 2

    try:
        json.loads(args.token)  # fail fast if not JSON
    except json.JSONDecodeError as exc:
        print(f"ERROR: token is not valid JSON: {exc}")
        return 2

    print(f"Connecting to {args.flight_url} ...")
    client = flight.connect(args.flight_url)

    options = make_options(args.token)
    command = f"{args.data_type}:{args.sample_id}"

    # 1) Unauthenticated call must fail (TM-02 check).
    try:
        client.get_flight_info(
            flight.FlightDescriptor.for_command(command.encode("utf-8"))
        )
        print("FAIL: unauthenticated get_flight_info unexpectedly succeeded")
        return 1
    except flight.FlightUnauthenticatedError:
        print("OK: unauthenticated request rejected (FlightUnauthenticatedError)")

    # 2) Authorized call.
    batches = fetch_batches(client, command, options)
    nrows = sum(b.num_rows for b in batches)
    print(f"OK: fetched {len(batches)} batch(es), {nrows} row(s) for '{command}'")
    for batch in batches:
        print(batch.schema)
        print(batch.to_pydict())

    return 0


if __name__ == "__main__":
    sys.exit(main())
