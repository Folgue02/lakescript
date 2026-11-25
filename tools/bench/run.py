#!/usr/bin/env python3
"""Times the benchmarks with hyperfine 2, in milliseconds, in CPU cycles and in
instructions. Cycles and instructions change less from run to run than the
time, which moves with the load of the machine: the release lks by default, another
lks given as the first argument, or the same programs in Python with
`--python`. From the repository:

    python3 tools/bench/run.py                    # target/release/lks
    python3 tools/bench/run.py path/to/lks fib    # only fib.lks
    python3 tools/bench/run.py --python

RUNS=10 changes how many times each one runs (5 by default).
"""
import json, os, subprocess, sys, tempfile
from pathlib import Path

here = Path(__file__).resolve().parent
args = sys.argv[1:]
python = '--python' in args
args = [a for a in args if a != '--python']
lks = args.pop(0) if args and not args[0].isidentifier() else 'target/release/lks'
only = set(args)
runs = os.environ.get('RUNS', '5')
folder, ext = (here / 'python', '.py') if python else (here, '.lks')
for script in sorted(folder.glob(f'*{ext}')):
    if only and script.stem not in only:
        continue
    command = f'python3 {script}' if python else f'{lks} --no-type-check {script}'
    with tempfile.NamedTemporaryFile(suffix='.json') as out:
        subprocess.run(['hyperfine', '-N', '-w', '1', '-r', runs, '--style', 'none', '--export-json', out.name, command],
                       check=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        s = json.load(open(out.name))['results'][0]['summary']
    wall = s['time_wall_clock']
    cycles, instructions = s.get('cpu_cycles', {}).get('mean', 0), s.get('instructions', {}).get('mean', 0)
    print(f"{script.stem:10} {wall['mean'] * 1000:8.1f} ms  ±{wall['stddev'] * 1000:5.1f}  {cycles / 1e6:7.0f} M cycles  {instructions / 1e6:7.0f} M instructions", flush=True)
