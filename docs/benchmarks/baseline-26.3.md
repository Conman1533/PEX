# Benchmark baseline: alpha against Paper (26.3)

Roadmap piece G0-03 (`docs/ROADMAP-2.0.md`, Goal 0). PEX at `alpha` `300621c3e` against Paper `26.3-169`, measured with the gameplay benchmark (github.com/Conman1533/mc-gameplay-bench, branch `fix/no-docker` at `1cb4d83`, which runs without Docker) in CT 117, 2026-11-02 20:04 UTC to 2026-11-03 04:13 UTC.

## Result

| Server | Runs | Tick P50 | Tick P95 | Tick P99 | CPU (cores) | Peak memory | Sent per bot-hour | P95 spread |
| :-- | --: | --: | --: | --: | --: | --: | --: | --: |
| PEX `300621c3e` | 3 | 12.05 ms | 22.95 ms | 39.54 ms | 0.40 | 361 MiB | 72.4 MiB | 5.7% |
| Paper 26.3-169 | 3 | 11.64 ms | 16.31 ms | 23.68 ms | 0.38 | 1,721 MiB | 52.4 MiB | 1.6% |

Each value is the mean over the runs, except peak memory, which is the highest of them. P95 spread is (highest run P95 minus lowest) divided by the mean P95.

The Paper row leaves out Paper's second run, whose tick P95 was 10% below the other three (see "Paper run 2" below). Over all four Paper runs: P50 11.45 ms, P95 15.88 ms, P99 22.90 ms, 0.38 cores, 52.1 MiB sent per bot-hour, P95 spread 11.8%.

## Every run

| Run | Tick P50 | Tick P95 | Tick P99 | Avg tick | CPU (cores) | Peak memory | Avg memory | Sent | Bot-hours | Sent per bot-hour | Report |
| :-- | --: | --: | --: | --: | --: | --: | --: | --: | --: | --: | :-- |
| pex-1 | 11.85 ms | 22.73 ms | 38.07 ms | 12.75 ms | 0.40 | 361 MiB | 275 MiB | 493.9 MiB | 6.30 | 78.4 MiB | `/srv/pex/scratch/bench-baseline/pex-1/report.html` |
| pex-2 | 12.83 ms | 23.71 ms | 41.32 ms | 13.74 ms | 0.42 | 335 MiB | 282 MiB | 409.2 MiB | 6.30 | 64.9 MiB | `/srv/pex/scratch/bench-baseline/pex-2/report.html` |
| pex-3 | 11.48 ms | 22.40 ms | 39.23 ms | 12.44 ms | 0.37 | 299 MiB | 258 MiB | 465.4 MiB | 6.30 | 73.8 MiB | `/srv/pex/scratch/bench-baseline/pex-3/report.html` |
| paper-1 | 11.72 ms | 16.29 ms | 23.21 ms | 12.20 ms | 0.37 | 1,721 MiB | 1,610 MiB | 308.5 MiB | 6.30 | 48.9 MiB | `/srv/pex/scratch/bench-baseline/paper-1/report.html` |
| paper-2 (left out) | 10.86 ms | 14.57 ms | 20.58 ms | 11.23 ms | 0.37 | 1,696 MiB | 1,607 MiB | 322.5 MiB | 6.30 | 51.2 MiB | `/srv/pex/scratch/bench-baseline/paper-2/report.html` |
| paper-3 | 11.56 ms | 16.45 ms | 25.06 ms | 12.01 ms | 0.37 | 1,635 MiB | 1,578 MiB | 346.3 MiB | 6.30 | 54.9 MiB | `/srv/pex/scratch/bench-baseline/paper-3/report.html` |
| paper-4 | 11.64 ms | 16.19 ms | 22.76 ms | 12.13 ms | 0.39 | 1,704 MiB | 1,634 MiB | 336.8 MiB | 6.30 | 53.4 MiB | `/srv/pex/scratch/bench-baseline/paper-4/report.html` |

Runs went in this order: pex-1, paper-1, pex-2, paper-2, paper-3, paper-4, pex-3. The report paths are in CT 117, each next to the run's `summary.txt`, raw samples (`run/`), bot logs (`bots/`), server log and `load.log`.

Confirmed actions per builder and miner bot, summed over all of its sessions (placed/broken):

| Run | Builder0 | Builder1 | Builder2 | Miner0 | Miner1 |
| :-- | --: | --: | --: | --: | --: |
| pex-1 | 705/701 | 622/615 | 737/737 | 0/1733 | 0/1562 |
| pex-2 | 790/786 | 625/625 | 730/730 | 0/1730 | 0/1561 |
| pex-3 | 744/744 | 625/625 | 747/747 | 0/1725 | 0/1561 |
| paper-1 | 792/787 | 653/652 | 676/670 | 0/1732 | 0/1565 |
| paper-2 | 809/805 | 700/696 | 755/752 | 0/1728 | 0/1563 |
| paper-3 | 809/807 | 654/650 | 738/736 | 0/1734 | 0/1565 |
| paper-4 | 809/805 | 711/710 | 748/742 | 0/1728 | 0/1565 |

Every run had 12 bot sessions (the fleet's seeded schedule), none ended early, and no bot died.

## Setup

- **One server at a time, on one CPU set.** Each run started one server as a systemd unit with `AllowedCPUs=22,23,24,27` and `MemoryMax=8G`. Those four CPUs are four different physical cores (cores 4, 5, 6 and 9 in `lscpu -e`), and none of their SMT siblings is in CT 117, so no benchmark process shared a core with the server. The sampler, the fleet and the 8 bots were pinned (`taskset`) to the CT's other 12 CPUs. Paper and PEX ran one after the other in alternating order, rather than side by side on two CPU sets, for three reasons. Both get exactly the same cores. Neither competes with the other for L3 cache and memory bandwidth. And later Goal 9 pieces will bench one PEX build at a time, which is only comparable with a baseline taken the same way.
- **Paper:** `/srv/pex/bins/paper` (`26.3-169`), JDK 25 (`/srv/pex/jdk-25`), `-Xms1G -Xmx4G` with the G1 flags of the G0-04 script, spark and bStats off, connection throttle off, `allow-flight=true`. Each start upgrades the 26.2 world copy to 26.3 (about 45 s before "Done").
- **PEX:** release build of `300621c3e` (`/srv/pex/scratch/bench-baseline/pumpkin-300621c3e`, sha256 `ee302191…`, the same file as `/srv/pex/target/bench-no-docker/release/pumpkin`; its dep-info files reference only the `300621c3e` worktree and it embeds that commit). Config from `/srv/pex/tools/pumpkin.zlib.toml` with spawn protection 0, whitelist on, telemetry and Bedrock off, `use_tty = false`, PEX's default storage settings.
- **Both:** a fresh copy of `/srv/pex/bins/base-world-262` per run, view and simulation distance 10, port 25611 on 127.0.0.1, 8 bots (3 builders, 2 miners, 2 AFK, 1 explorer) on the fleet's seeded schedule for 60 minutes, sampler every 10 s.
- **Quiet machine.** Every run went through `/srv/pex/tools/build-slot.sh`, and `load.log` in each run directory has a line a minute: host load, CT 117's total CPU, the server's CPU, cargo and rustc processes, the CI units and the build slots. In all seven runs the other two slots stayed free, no cargo or rustc ran, and CT 117's CPU apart from the server stayed at or under 0.13 cores (the bots). The CI poll timer ran at :00, :15, :30 and :45 and started nothing each time (`journalctl -u pex-ci-alpha`). The 03:00 nightly fell inside paper-4 and exited at once (no stages in `alpha-nightly.d`). The load average CT 117 shows is the host's (lxcfs doesn't virtualize it): it averaged 5 to 9 per run with peaks up to 35 (pex-3) from other guests on the host.
- Scripts: `/srv/pex/scratch/bench-baseline/run-baseline.sh <out_dir> <minutes> <paper|pex> <pex_binary>` (a copy of the G0-04 script for one server per run), `chain.sh` (runs them in sequence), `analyze.py` (per-run numbers and bot-hours), `tables.py` (these tables).

## How the numbers are made

- **Tick P50/P95/P99:** the sampler sends `tick query` every 30 s, and each answer gives the percentiles of the last 100 ticks. `report.py` reports the mean of those answers over the run, the idle minutes before the first bot joins included. Both servers answer from a 100-tick window (vanilla's size).
- **CPU:** the server cgroup's CPU use (`cpu.stat` `usage_usec`) per 10-second sample, averaged over the run, in cores.
- **Memory:** the server cgroup's `memory.current` (anonymous memory, page cache and kernel memory) every 10 s: the highest sample and the mean.
- **Sent:** nftables counters on the game port from the sampler's first to its last sample (IP packets, IP and TCP headers included, no Ethernet header). **Bot-hours** are the sum of every bot session, from its `joined` line to its last line in the bot's log; 6.30 in every run because the schedule is seeded. Sent per bot-hour is one divided by the other.

## Notes on the numbers

- **Paper run 2.** Its P95 was 14.57 ms, against 16.19 to 16.45 ms in the other three Paper runs. In 10-minute blocks it ran lower from minute 10 to 40. Nothing on the machine explains it: the CT was as quiet as in the other runs and the host load averaged 6.9, as in paper-4. One known difference between runs is the explorer bot: `survbot.py` picks its heading with an unseeded `random`, so it walks into different, newly generated terrain each run (the fleet's schedule is seeded, the bots' movement isn't). In paper-2 it went the farthest, 644 blocks, and toward +Z, where no other run went. That fits but isn't shown.
- **Network varies more than tick time between runs** for the same reason: PEX sent 409 to 494 MiB, Paper 309 to 346 MiB.
- **The bots ask for view distance 8**, so the server's view distance of 10 is a cap they never reach (see Goal 9's Current state).
- These numbers aren't comparable with the 2026-10-14 results in the benchmark's README: those ran both servers in Docker on a Pelican node and counted the container's whole traffic.
