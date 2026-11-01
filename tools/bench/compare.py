#!/usr/bin/env python3
"""Download one torrent with several engines and compare time, CPU and memory.

Each round runs every engine once, one at a time, in a rotated order, from an
empty directory. Writes JSON and a Markdown table. Results depend on the swarm:
compare CPU and memory more than time, and use several rounds.
"""
import argparse
import json
import os
import shutil
import signal
import statistics
import subprocess
import tempfile
import time
from pathlib import Path

LT_SCRIPT = """
import sys, time, libtorrent as lt
s = lt.session({'listen_interfaces': '0.0.0.0:0'})
h = s.add_torrent({'ti': lt.torrent_info(sys.argv[1]), 'save_path': sys.argv[2]})
while not h.status().is_seeding:
    time.sleep(0.2)
"""


def commands(args, out):
    torrent = str(args.torrent)
    return {
        'ziyron': [args.ziyron, 'download', torrent, '-o', out],
        'rqbit': [args.rqbit, 'download', '-e', '-o', out, torrent],
        'libtorrent': [args.libtorrent_python, '-c', LT_SCRIPT, torrent, out],
        'aria2': ['aria2c', '--seed-time=0', '--dir', out, '--console-log-level=error', '--summary-interval=0', torrent],
        'webtorrent': [args.webtorrent, 'download', torrent, '-o', out, '--quiet'],
    }


def run_once(name, args):
    out = tempfile.mkdtemp(prefix=f'{name}-', dir=args.work)
    start = time.monotonic()
    proc = subprocess.Popen(commands(args, out)[name], stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL,
                            stderr=subprocess.DEVNULL, start_new_session=True)
    deadline = start + args.timeout
    while True:
        pid, status, usage = os.wait4(proc.pid, os.WNOHANG)
        if pid:
            break
        if time.monotonic() > deadline:
            os.killpg(proc.pid, signal.SIGKILL)
            pid, status, usage = os.wait4(proc.pid, 0)
            status = -1
            break
        time.sleep(0.05)
    seconds = time.monotonic() - start
    size = sum(f.stat().st_size for f in Path(out).rglob('*') if f.is_file() and '.ziyron' not in f.parts)
    shutil.rmtree(out, ignore_errors=True)
    return dict(engine=name, ok=status == 0, seconds=round(seconds, 1), cpu=round(usage.ru_utime + usage.ru_stime, 2),
                peak_mib=round(usage.ru_maxrss / 1024, 1), bytes=size)


def table(results, engines):
    rows = ['| Engine | Finished | Time (s) | CPU time (s) | Peak RAM (MiB) |', '|---|---:|---:|---:|---:|']
    for name in engines:
        runs = [r for r in results if r['engine'] == name]
        good = [r for r in runs if r['ok']]
        if not good:
            rows.append(f'| {name} | 0/{len(runs)} | - | - | - |')
            continue
        med = lambda key: statistics.median(r[key] for r in good)
        rows.append(f"| {name} | {len(good)}/{len(runs)} | {med('seconds'):.1f} | {med('cpu'):.2f} | {med('peak_mib'):.0f} |")
    return '\n'.join(rows)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('torrent', type=Path)
    parser.add_argument('--ziyron', default='target/release/ziyron')
    parser.add_argument('--rqbit', default='rqbit')
    parser.add_argument('--libtorrent-python', default='python3')
    parser.add_argument('--webtorrent', default='webtorrent')
    parser.add_argument('--engines', default='ziyron,rqbit,libtorrent,aria2,webtorrent')
    parser.add_argument('--rounds', type=int, default=3)
    parser.add_argument('--timeout', type=int, default=300)
    parser.add_argument('--work', default=None)
    parser.add_argument('--out', type=Path, default=Path('bench-results.json'))
    args = parser.parse_args()
    args.work = args.work or tempfile.mkdtemp(prefix='ziyron-bench-')
    os.makedirs(args.work, exist_ok=True)
    engines = args.engines.split(',')
    results = []
    for round_index in range(args.rounds):
        order = engines[round_index % len(engines):] + engines[:round_index % len(engines)]
        for name in order:
            result = run_once(name, args)
            result['round'] = round_index
            results.append(result)
            print(result, flush=True)
            args.out.write_text(json.dumps(results, indent=1))
    print(table(results, engines))


if __name__ == '__main__':
    main()
