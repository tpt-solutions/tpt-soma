"""Checks the in-house Flight implementation against a real Arrow client.

Run the server first:
    cargo run -p tpt-soma-flight --example interop_server -- 127.0.0.1:50999
then:
    python scripts/flight_interop_check.py 127.0.0.1:50999

Needs `pip install pyarrow`. Exits non-zero on any mismatch.
"""

import json
import sys

import pyarrow as pa
import pyarrow.flight as fl

addr = sys.argv[1] if len(sys.argv) > 1 else "127.0.0.1:50999"
client = fl.connect(f"grpc://{addr}")

expected = pa.table(
    {
        "id": pa.array(["a", "bb", ""], pa.string()),
        "pos": pa.array([1, -2, 3], pa.int32()),
        "ts": pa.array([10, 20, 2**63 - 1], pa.int64()),
        "val": pa.array([1.5, None, -0.25], pa.float64()),
        "note": pa.array(["x", None, "héllo"], pa.string()),
        "flag": pa.array([True, False, None], pa.bool_()),
    },
    schema=pa.schema(
        [
            pa.field("id", pa.string(), nullable=False),
            pa.field("pos", pa.int32(), nullable=False),
            pa.field("ts", pa.int64(), nullable=False),
            pa.field("val", pa.float64()),
            pa.field("note", pa.string()),
            pa.field("flag", pa.bool_()),
        ]
    ),
)

# GetFlightInfo: schema and endpoint decode.
info = client.get_flight_info(fl.FlightDescriptor.for_command(b"variants:abc"))
assert info.schema.equals(expected.schema), f"schema mismatch:\n{info.schema}\n{expected.schema}"
assert info.total_records == -1 and info.total_bytes == -1
assert info.endpoints[0].ticket.ticket == b"variants:abc"
print("get_flight_info ok")

# DoGet: pyarrow reassembles and validates the stream.
got = client.do_get(info.endpoints[0].ticket).read_all()
got.validate(full=True)
assert got.equals(expected), f"data mismatch:\n{got}\n{expected}"
print("do_get ok:", got.num_rows, "rows")

# DoPut: pyarrow encodes, the server decodes and reports the row count.
writer, reader = client.do_put(fl.FlightDescriptor.for_command(b"device_state"), expected.schema)
writer.write_table(expected)
writer.done_writing()
result = reader.read()
writer.close()
assert json.loads(result.to_pybytes())["rows"] == 3, result.to_pybytes()
print("do_put ok:", result.to_pybytes().decode())

# Unimplemented calls must fail cleanly rather than hang.
try:
    list(client.list_flights())
except pa.ArrowNotImplementedError:
    print("list_flights unimplemented ok")
print("ALL OK")
