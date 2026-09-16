#!/usr/bin/env python3
"""Compare preserved and current release binaries on fixed, offline workloads.

Requires GNU time (uses a small wrapper process for accurate peak RSS). Run from the repo root:
  python3 scripts/benchmark-wasm-grammars.py --baseline dist/wasm-grammar-baseline/treetags \
      --plugins dist/wasm-grammar-baseline/plugins --current target/release/treetags
Both binaries must have been built with the same toolchain, target and profile.
Caches are isolated per binary; use --cache cold for empty-cache processes or
--cache disabled for uncached compilation. The default primes the disk cache.
Zig and mixed cases explicitly request access, end, and signature fields from
both binaries to match the old plugin's unconditional Zig field emission.
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


def measure(command, cwd, env, output_dir):
    stdout = output_dir / "stdout"
    stderr = output_dir / "stderr"
    rss_file = output_dir / "rss"
    with stdout.open("wb") as out, stderr.open("wb") as err:
        start = time.perf_counter()
        # GNU time forks the target from its own small process, so Python's
        # fixture/hash buffers cannot inflate the target's peak RSS.
        proc = subprocess.run(["/usr/bin/time", "-f", "%M", "-o", str(rss_file), *command],
                              cwd=cwd, env=env, stdout=out, stderr=err)
        elapsed = time.perf_counter() - start
    if proc.returncode or stderr.stat().st_size:
        raise RuntimeError(f"{command}: {proc.returncode}: {stderr.read_text()}")
    rss_kib = int(rss_file.read_text().strip())
    return elapsed, rss_kib, hashlib.sha256(stdout.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--baseline", type=Path, required=True)
    parser.add_argument("--current", type=Path, required=True)
    parser.add_argument("--plugins", type=Path, required=True, help="Preserved plugin directory")
    parser.add_argument("--runs", type=int, default=7)
    parser.add_argument("--cache", choices=["disabled", "cold", "warm"], default="warm",
                        help="Isolated compilation cache state (default: warm)")
    parser.add_argument("--output", type=Path, default=Path("dist/wasm-grammar-benchmark.json"))
    args = parser.parse_args()
    if args.runs < 2:
        parser.error("--runs must be at least 2")
    repo = Path(__file__).resolve().parent.parent
    binaries = {"before": args.baseline.resolve(), "after": args.current.resolve()}
    plugins = args.plugins.resolve()
    fixtures = {
        "native": ("rs", (repo / "tests/test_cases/rust/basic/input/source.rs").read_bytes()),
        "zig": ("zig", (repo / "tests/test_cases/zig/basic/input/source.zig").read_bytes()),
        "ocaml": ("ml", (repo / "tests/test_cases/ocaml/basic/input/source.ml").read_bytes()),
    }
    result = {"platform": platform.platform(), "runs": args.runs, "cache": args.cache,
              "warmups": 1, "large_files_per_language": 40, "copies_per_large_file": 50,
              "fields_for_zig_workloads": "+a,+e,+S",
              "binaries": {}, "workloads": []}
    for label, binary in binaries.items():
        result["binaries"][label] = {"path": str(binary), "bytes": binary.stat().st_size,
                                     "sha256": hashlib.sha256(binary.read_bytes()).hexdigest()}
    with tempfile.TemporaryDirectory(prefix="treetags-benchmark-") as temporary:
        root = Path(temporary)
        config = root / "config"
        shutil.copytree(repo / "tests/grammars/wasm/14", config / "treetags/wasm_grammars/14")
        env = dict(os.environ, XDG_CONFIG_HOME=str(config))
        caches = {label: root / f"cache-{label}" for label in binaries}
        if args.cache == "disabled":
            for cache in caches.values():
                cache.write_text("cache unavailable")
        empty_config = root / "empty.toml"
        empty_config.write_text("user_grammars = []\n")
        cases = [("startup", [])]
        for language in ["native", "zig", "ocaml", "mixed"]:
            langs = list(fixtures) if language == "mixed" else [language]
            for size, count, copies in [("small", 1, 1), ("large", 40, 50)]:
                directory = root / f"{language}-{size}"
                directory.mkdir()
                for lang in langs:
                    extension, source = fixtures[lang]
                    for number in range(count):
                        (directory / f"source{number}.{extension}").write_bytes(source * copies)
                cases.append((directory.name, langs))
        (root / "startup").mkdir()
        for name, langs in cases:
            for workers in [1, 4]:
                commands = {}
                for label, binary in binaries.items():
                    commands[label] = [str(binary), "--plugins-dir", str(root / "empty-plugins"),
                                       "--user-languages-config", str(empty_config),
                                       "--workers", str(workers), "-f", "-", "."]
                    if label == "before" and "zig" in langs:
                        commands[label] += ["--plugin-dir", str(plugins / "zig")]
                    if "zig" in langs:
                        # The old Zig plugin emitted these unconditionally.
                        commands[label] += ["--fields", "+a,+e,+S"]
                samples = {label: [] for label in binaries}
                hashes = set()
                # Alternate order to reduce systematic thermal/cache bias.
                for iteration in range(args.runs + 1):
                    for label in list(binaries)[::1 if iteration % 2 == 0 else -1]:
                        cache = caches[label]
                        if args.cache == "cold" and cache.exists():
                            shutil.rmtree(cache)
                        run_env = dict(env, XDG_CACHE_HOME=str(cache))
                        seconds, rss, digest = measure(commands[label], root / name, run_env, root)
                        hashes.add(digest)
                        if iteration:
                            samples[label].append({"seconds": seconds, "peak_rss_kib": rss})
                if len(hashes) != 1:
                    raise RuntimeError(f"Output differs for {name} with {workers} workers")
                inputs = list((root / name).iterdir())
                row = {"name": name, "workers": workers, "output_sha256": hashes.pop(),
                       "input_files": len(inputs), "input_bytes": sum(p.stat().st_size for p in inputs)}
                for label, values in samples.items():
                    durations = [v["seconds"] for v in values]
                    row[label] = {"median_seconds": statistics.median(durations),
                                  "min_seconds": min(durations), "max_seconds": max(durations),
                                  "median_peak_rss_kib": statistics.median(v["peak_rss_kib"] for v in values),
                                  "samples": values}
                result["workloads"].append(row)
                print(f"{name:14} workers={workers}: {row['before']['median_seconds']:.4f}s -> "
                      f"{row['after']['median_seconds']:.4f}s", flush=True)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(result, indent=2) + "\n")


if __name__ == "__main__":
    main()
