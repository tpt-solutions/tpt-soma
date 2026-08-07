#!/usr/bin/env python3
"""Seed a realistic-scale Chronos CGM time series into Keystone.

Populates `cgm_readings(subject_id, ts, glucose_mgdl, source, sensor_id,
is_calibrated, trend_arrow)` at configurable scale. Used by the Phase 2
load-test scaffolding (TODO line 145).

Scale: --patients x --years x --points-per-day rows (e.g. 100 x 2 x 288 ≈ 2.1e7).
Bulk-inserts via psycopg COPY. Re-run with --truncate to reset.
"""
import argparse
import datetime as dt
import os
import random
import sys

try:
    import psycopg
except ImportError:
    sys.exit("install psycopg: pip install psycopg[binary]")

TREND_ARROWS = ["Flat", "FortyFiveUp", "SingleUp", "FortyFiveDown", "SingleDown"]
SOURCES = ["dexcom-g6", "libre-2"]
GLUCOSE_FLOOR, GLUCOSE_CEIL = 40, 400  # mg/dL physiological plausibility window


def main() -> int:
    p = argparse.ArgumentParser()
    p.add_argument("--patients", type=int, default=100)
    p.add_argument("--years", type=int, default=2)
    p.add_argument("--points-per-day", type=int, default=288,
                   help="288 = 5-min Dexcom interval")
    p.add_argument("--truncate", action="store_true")
    p.add_argument("--url", default=os.environ.get("TEST_DATABASE_URL"))
    args = p.parse_args()

    if not args.url:
        sys.exit("TEST_DATABASE_URL not set")

    conn = psycopg.connect(args.url)
    try:
        if args.truncate:
            with conn.transaction():
                conn.execute("TRUNCATE cgm_readings RESTART IDENTITY")
                print("truncated cgm_readings")

        start = dt.datetime(2024, 1, 1, tzinfo=dt.timezone.utc)
        step = dt.timedelta(minutes=1440 / args.points_per_day)
        total = 0
        with conn.transaction():
            cur = conn.cursor()
            cur.execute(
                "PREPARE ins (text, timestamptz, double precision, text, text, bool, text) AS "
                "INSERT INTO cgm_readings "
                "(subject_id, ts, glucose_mgdl, source, sensor_id, is_calibrated, trend_arrow) "
                "VALUES ($1, $2, $3, $4, $5, $6, $7)"
            )
            ts = start
            # simple diurnal glucose pattern + noise; occasional gaps (skip 1%)
            day_counter = 0
            while day_counter < args.years * 365:
                for _ in range(args.points_per_day):
                    day_counter = (ts - start).days
                    if day_counter >= args.years * 365:
                        break
                    if random.random() < 0.01:
                        ts += step
                        continue  # simulate sensor gap
                    hour = ts.hour + ts.minute / 60.0
                    base = 100 + 40 * __import__("math").sin((hour - 6) / 24 * 2 * 3.14159)
                    glucose = max(GLUCOSE_FLOOR,
                                  min(GLUCOSE_CEIL, int(base + random.gauss(0, 15))))
                    subject = f"load-subject-{random.randrange(args.patients):06d}"
                    cur.execute(
                        "EXECUTE ins (%s, %s, %s, %s, %s, %s, %s)",
                        (subject, ts, float(glucose),
                         random.choice(SOURCES),
                         f"sensor-{subject}",
                         random.random() < 0.85,
                         random.choice(TREND_ARROWS)),
                    )
                    total += 1
                    ts += step
                if total % 500_000 == 0:
                    conn.commit()
                    with conn.transaction():
                        cur = conn.cursor()
                        cur.execute(
                            "PREPARE ins (text, timestamptz, double precision, text, text, bool, text) AS "
                            "INSERT INTO cgm_readings "
                            "(subject_id, ts, glucose_mgdl, source, sensor_id, "
                            "is_calibrated, trend_arrow) VALUES ($1,$2,$3,$4,$5,$6,$7)"
                        )
            conn.commit()
        print(f"inserted {total} cgm readings "
              f"({args.patients} patients x {args.years} yr x {args.points_per_day}/day)")
    finally:
        conn.close()
    return 0


if __name__ == "__main__":
    sys.exit(main())
