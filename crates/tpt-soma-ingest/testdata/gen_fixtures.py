"""Generate minimal synthetic .h5ad fixtures for the AnnDataParser tests.

The fixtures intentionally mirror the subset of the AnnData-on-disk layout that
`crates/tpt-soma-ingest/src/h5ad.rs` reads:

  - `obs/_index`: fixed-size ASCII string dataset of cell ids
  - `var/_index`: fixed-size ASCII string dataset of gene ids
  - `X/X`       : dense float64 expression matrix (dense fixture)
  - `X/data`, `X/indices`, `X/indptr`: CSR components (sparse fixture)

Run from the repository root:

    python crates/tpt-soma-ingest/testdata/gen_fixtures.py

Requires numpy and h5py (pip install numpy h5py). Deterministic output; the
resulting files are committed so Rust tests need no HDF5 toolchain.
"""

from pathlib import Path

import h5py
import numpy as np

OUT_DIR = Path(__file__).resolve().parent


def write_strings(f, path, values):
    """Fixed-size ASCII (null-padded) string dataset, readable by hdf5-reader."""
    max_len = max(len(v) for v in values)
    dt = np.dtype(f"S{max_len}")
    f.create_dataset(path, data=np.array(values, dtype=dt))


def gen_dense():
    cells = ["cell-1", "cell-2", "cell-3"]
    genes = ["BRCA1", "TP53", "GAPDH", "ACTB"]
    x = np.array(
        [
            [10.0, 0.0, 200.0, 0.0],
            [5.0, 3.0, 0.0, 1.0],
            [0.0, 0.0, 100.0, 4.0],
        ],
        dtype=np.float64,
    )
    out = OUT_DIR / "mini_scrna_dense.h5ad"
    with h5py.File(out, "w") as f:
        obs = f.create_group("obs")
        write_strings(f, "obs/_index", cells)
        var = f.create_group("var")
        write_strings(f, "var/_index", genes)
        xg = f.create_group("X")
        xg.create_dataset("X", data=x)
    print(f"wrote {out} ({x.size} dense values, {x.nonzero()[0].size} non-zero)")


def gen_sparse():
    # 4 cells so indptr has 5 entries (20 bytes, int32) -- avoids the parser's
    # 8-byte/4-byte width heuristic ambiguity for this fixture.
    cells = [f"cell-{i}" for i in range(1, 5)]
    genes = ["BRCA1", "TP53", "GAPDH", "ACTB", "MYC"]
    indptr = np.array([0, 2, 4, 5, 7], dtype=np.int32)
    indices = np.array([0, 2, 0, 1, 4, 3, 0], dtype=np.int32)
    data = np.array([10.0, 200.0, 5.0, 3.0, 1.0, 4.0, 100.0], dtype=np.float32)
    out = OUT_DIR / "mini_scrna_sparse.h5ad"
    with h5py.File(out, "w") as f:
        write_strings(f, "obs/_index", cells)
        write_strings(f, "var/_index", genes)
        xg = f.create_group("X")
        xg.create_dataset("data", data=data)
        xg.create_dataset("indices", data=indices)
        xg.create_dataset("indptr", data=indptr)
    print(f"wrote {out} ({indices.size} sparse entries)")


if __name__ == "__main__":
    gen_dense()
    gen_sparse()
