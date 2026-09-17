#!/usr/bin/env python3
"""Measure OCaml compilation-cache overhead in fresh release processes.

Uses the same binary with an unavailable, empty, or populated cache. Requires
GNU time. Outputs must match in every run; timings are not test assertions.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import shutil
import statistics
import subprocess
import tempfile
import time


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=Path("target/release/treetags"))
    parser.add_argument("--runs", type=int, default=7)
    parser.add_argument("--output", type=Path, default=Path("dist/grammar-cache-benchmark.json"))
    args = parser.parse_args()
    if args.runs < 2:
        parser.error("--runs must be at least 2")
    binary = args.binary.resolve()
    repo = Path(__file__).resolve().parent.parent
    results = {"platform": platform.platform(), "runs": args.runs,
               "binary_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
               "workloads": []}
    with tempfile.TemporaryDirectory(prefix="treetags-cache-benchmark-") as tmp:
        root = Path(tmp)
        config = root / "config"
        shutil.copytree(repo / "tests/grammars/wasm/14", config / "treetags/wasm_grammars/14")
        source = (repo / "tests/test_cases/ocaml/basic/input/source.ml").read_bytes()
        # A file in place of the cache directory exercises uncached compilation.
        (root / "disabled").write_text("cache unavailable")
        for size, count, copies in [("small", 1, 1), ("large", 40, 50)]:
            project = root / size
            project.mkdir()
            for i in range(count):
                (project / f"source{i}.ml").write_bytes(source * copies)
            for workers in [1, 4]:
                samples = {mode: [] for mode in ["disabled", "cold", "warm"]}
                expected = None
                # One unmeasured iteration primes filesystem and warm disk caches.
                for iteration in range(args.runs + 1):
                    modes = list(samples)
                    if iteration % 2:
                        modes.reverse()
                    for mode in modes:
                        cache = root / mode
                        if mode == "cold" and cache.exists():
                            shutil.rmtree(cache)
                        env = dict(os.environ, XDG_CONFIG_HOME=str(config), XDG_CACHE_HOME=str(cache))
                        command = ["/usr/bin/time", "-f", "%M", "-o", str(root / "rss"),
                                   str(binary), "--plugins-dir", str(root / "no-plugins"),
                                   "--workers", str(workers), "-f", "-"]
                        start = time.perf_counter()
                        out = subprocess.run(command, cwd=project, env=env, capture_output=True, check=True)
                        elapsed = (time.perf_counter() - start) * 1000
                        if out.stderr:
                            raise RuntimeError(out.stderr.decode())
                        digest = hashlib.sha256(out.stdout).hexdigest()
                        if expected is None:
                            expected = digest
                        assert digest == expected, "tag output differs"
                        if iteration:
                            samples[mode].append({"ms": elapsed, "rss_kib": int((root / "rss").read_text())})
                for mode, runs in samples.items():
                    entry = {"size": size, "workers": workers, "cache": mode,
                             "median_ms": statistics.median(r["ms"] for r in runs),
                             "median_rss_mib": statistics.median(r["rss_kib"] for r in runs) / 1024,
                             "output_sha256": expected, "samples": runs}
                    results["workloads"].append(entry)
                    print(f"{size:5} workers={workers} {mode:8} {entry['median_ms']:.1f} ms "
                          f"{entry['median_rss_mib']:.1f} MiB", flush=True)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(results, indent=2) + "\n")


if __name__ == "__main__":
    main()
