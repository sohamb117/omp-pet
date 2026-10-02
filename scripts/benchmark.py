#!/usr/bin/env python3
"""Benchmark a running release app. No third-party dependencies or model calls.
Temporarily changes displayed activity and tucking; restores visibility at the end.
CPU is delta process CPU time / wall time (100% = one CPU core); RSS includes shared pages.
"""
import argparse
import json
import os
import platform
import socket
import statistics
import subprocess
import time
from pathlib import Path


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--seconds', type=float, default=20, help='seconds per scenario')
    parser.add_argument('--output', type=Path, default=Path('work/benchmark.json'))
    parser.add_argument('--socket', default=os.environ.get('OMP_PET_SOCKET', f'/tmp/omp-pet-{os.getuid()}/events.sock'))
    args = parser.parse_args()
    if args.seconds < 5:
        parser.error('Use at least 5 seconds per scenario')

    def control(command):
        with socket.socket(socket.AF_UNIX) as client:
            client.settimeout(5)
            client.connect(args.socket)
            client.sendall((json.dumps({'control': command}) + '\n').encode())
            result = json.loads(client.makefile().readline())
            if not result.get('ok'):
                raise RuntimeError(result)
            return result

    before = control('status')
    if not before.get('process_usage'):
        raise SystemExit('Rebuild/restart the app with process counters first')
    report = {'recorded_at': time.strftime('%Y-%m-%dT%H:%M:%S%z'),
              'platform': platform.platform(), 'architecture': platform.machine(),
              'commit': subprocess.check_output(['git', 'rev-parse', 'HEAD'], text=True).strip(),
              'sprite_pack': Path(before['sprites']).name if before['sprites'] else 'native cat',
              'widget_size': before['sprite_viewbox']['width'], 'scenarios': [],
              'method': 'Release process: delta user+system CPU / monotonic wall time; 1 Hz self-reported Mach RSS. 100% CPU = one core. Includes status polling overhead. RSS includes shared pages; this is not private footprint, GPU, energy, or whole-system usage.'}
    seq = 0
    with socket.socket(socket.AF_UNIX) as feed:
        feed.connect(args.socket)
        def snapshot(activity):
            nonlocal seq
            seq += 1
            feed.sendall((json.dumps(dict(version=1, session_id='resource-benchmark', seq=seq,
                project='Benchmark', task='Measuring native pet resource usage', activity=activity,
                tool='Synthetic tool step' if activity == 'working' else None,
                context=dict(tokens=42000, window=100000, percent=42))) + '\n').encode())
        try:
            for name, activity, visibility in [('visible_idle', 'idle', 'show_pet'),
                    ('visible_working', 'working', 'show_pet'),
                    ('readout_working', 'working', 'readout'),
                    ('tucked_working', 'working', 'tuck')]:
                snapshot(activity)
                control(visibility)
                time.sleep(1)  # Exclude transition/decode warm-up from measurement.
                first = control('status')
                start = time.monotonic()
                samples = []
                latencies = []
                while time.monotonic() - start < args.seconds:
                    time.sleep(min(1, args.seconds - (time.monotonic() - start)))
                    if activity == 'working':
                        snapshot(activity)
                    sent = time.monotonic()
                    last = control('status')
                    latencies.append((time.monotonic() - sent) * 1000)
                    samples.append(last['process_usage']['resident_bytes'] / 1024**2)
                elapsed = time.monotonic() - start
                cpu = (last['process_usage']['cpu_seconds'] - first['process_usage']['cpu_seconds']) / elapsed * 100
                result = dict(scenario=name, seconds=round(elapsed, 2), cpu_percent_one_core=round(cpu, 3),
                    rss_mib_mean=round(statistics.mean(samples), 2), rss_mib_peak=round(max(samples), 2),
                    ipc_ms_median=round(statistics.median(latencies), 2),
                    expected_state_observed=last['tucked'] == (visibility == 'tuck') and last['activity'] == activity,
                    animation_running=last['animation_running'], edge_watch=last['edge_watch'])
                report['scenarios'].append(result)
                print(json.dumps(result), flush=True)
        finally:
            snapshot('disconnected')
            control('tuck' if before['tucked'] else 'readout' if before['readout_pinned'] else 'show_pet')
    binary = Path('dist/OMP Pet.app/Contents/MacOS/omp-pet')
    report['binary_bytes'] = binary.stat().st_size
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, indent=2) + '\n')
    print(f'Saved {args.output}', flush=True)


if __name__ == '__main__':
    main()
