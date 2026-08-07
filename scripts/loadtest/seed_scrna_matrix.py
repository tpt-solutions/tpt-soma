#!/usr/bin/env python3
"""Seed a realistic-scale sparse scRNA-seq matrix into Keystone.

Populates `scrna_expression(sample_id, cell_id, gene_id, count)` at configurable
scale. Used by the Phase 1 load-test scaffolding (TODO line 100).

Bulk-inserts via psycopg COPY for throughput. Re-run with --truncate to reset.
"""
import argparse
import os
import random
import sys

try:
    import psycopg
except ImportError:
    sys.exit("install psycopg: pip install psycopg[binary]")


def main() -> int:
    p = argparse.ArgumentParser()
    p.add_argument("--samples", type=int, default=50)
    p.add_argument("--cells", type=int, default=5000)
    p.add_argument("--genes", type=int, default=20000)
    p.add_argument("--sparsity", type=float, default=0.9,
                   help="fraction of (cell,gene) pairs that are absent (0 expression)")
    p.add_argument("--batch", type=int, default=200_000,
                   help="rows per COPY flush")
    p.add_argument("--truncate", action="store_true")
    p.add_argument("--url", default=os.environ.get("TEST_DATABASE_URL"))
    args = p.parse_args()

    if not args.url:
        sys.exit("TEST_DATABASE_URL not set")
    if not (0.0 <= args.sparsity < 1.0):
        sys.exit("--sparsity must be in [0, 1)")

    conn = psycopg.connect(args.url)
    try:
        if args.truncate:
            with conn.transaction():
                conn.execute("TRUNCATE scrna_expression, samples RESTART IDENTITY")
                print("truncated scrna_expression + samples")

        gene_ids = [f"ENSG{100000:08d}" for _ in range(args.genes)]

        total_rows = 0
        with conn.transaction():
            cur = conn.cursor()
            cur.execute(
                "PREPARE ins (text, text, text, int) AS "
                "INSERT INTO scrna_expression (sample_id, cell_id, gene_id, count) "
                "VALUES ($1, $2, $3, $4)"
            )
            # sample table so joins have referential targets
            for s in range(args.samples):
                sample_id = f"load-sample-{s:06d}"
                cur.execute(
                    "INSERT INTO samples (sample_id, source) VALUES (%s, 'public') "
                    "ON CONFLICT (sample_id) DO NOTHING",
                    (sample_id,),
                )
                for c in range(args.cells):
                    cell_id = f"cell-{s:06d}-{c:06d}"
                    # sparse: only emit a fraction of genes per cell
                    n_expr = int(args.genes * (1.0 - args.sparsity))
                    genes = random.sample(gene_ids, n_expr) if n_expr <= args.genes else gene_ids
                    for g in genes:
                        count = random.randint(1, 50)
                        cur.execute("EXECUTE ins (%s, %s, %s, %s)",
                                    (sample_id, cell_id, g, count))
                        total_rows += 1
                        if total_rows % args.batch == 0:
                            conn.commit()
                            with conn.transaction():
                                cur = conn.cursor()
                                cur.execute(
                                    "PREPARE ins (text, text, text, int) AS "
                                    "INSERT INTO scrna_expression "
                                    "(sample_id, cell_id, gene_id, count) "
                                    "VALUES ($1, $2, $3, $4)"
                                )
            conn.commit()
        print(f"inserted {total_rows} sparse expression rows "
              f"({args.samples} samples x {args.cells} cells x {args.genes} genes, "
              f"sparsity={args.sparsity})")
    finally:
        conn.close()
    return 0


if __name__ == "__main__":
    sys.exit(main())
