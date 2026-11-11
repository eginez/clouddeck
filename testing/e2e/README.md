# End-to-end screenshots

Runs the real app against the local emulators (`testing/emulators`) and compares each screenshot with `scenarios/<name>/expected/main.png`.
Dry run, no Docker or app needed: `testing/e2e/run.sh --dry-run`.
Capture and compare (starts the emulators unless `--no-up`; outputs in `/tmp/clouddeck-e2e`, not committed): `testing/e2e/run.sh [outdir]`.
Approve the current screenshot as expected: `testing/e2e/run.sh --update [scenario]` (put `outdir` before `--update`).
Hand-made mockups in `expected/` will FAIL with a large percentage because they cannot match the real render pixel for pixel; look at `actual.png` and `diff.png`, and only trust FAIL once `expected/` holds screenshots taken with `--update`.
