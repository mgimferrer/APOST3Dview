#!/bin/bash
# Runs the six ORCA jobs and writes a .molden file for each.
#
# With ORCA's module loaded, just run ./run_all.sh in this folder. Otherwise
# set ORCA_DIR to the folder holding orca and orca_2mkl, e.g.
#   ORCA_DIR=/opt/orca-6.1.0 ./run_all.sh
# NPROCS sets the cores per job (ORCA's %pal) and MAXCORE the memory per
# core in MB (ORCA's %maxcore), both written into each input:
#   NPROCS=16 MAXCORE=3000 ./run_all.sh
# ORCA is called with its full path, as its parallel runs require. A failed
# job is reported and the others still run.
ORCA_DIR=${ORCA_DIR:-$(dirname "$(command -v orca)")}
for program in orca orca_2mkl; do
    if [ ! -x "$ORCA_DIR/$program" ]; then
        echo "$program not found in '$ORCA_DIR': load ORCA's module or set ORCA_DIR" >&2
        exit 1
    fi
done
echo "Using $ORCA_DIR/orca"

# Cores and memory per job: any earlier %pal/%maxcore line is replaced by
# the requested one (or removed), right after the "!" line.
for input in *.inp; do
    awk -v nprocs="$NPROCS" -v maxcore="$MAXCORE" '
        /^%pal/ || /^%maxcore/ { next }
        { print }
        /^!/ {
            if (nprocs > 1) print "%pal nprocs " nprocs " end"
            if (maxcore != "") print "%maxcore " maxcore
        }
    ' "$input" > "$input.tmp" && mv "$input.tmp" "$input"
done
echo "Cores per job: ${NPROCS:-1}; memory per core: ${MAXCORE:-default} MB"

failed=()
for input in *.inp; do
    job=${input%.inp}
    echo "== $job"
    if ! "$ORCA_DIR/orca" "$input" > "$job.out" 2>&1 || ! grep -q "ORCA TERMINATED NORMALLY" "$job.out"; then
        echo "   failed, see $job.out" >&2
        failed+=("$job")
        continue
    fi
    # orca_2mkl writes <job>.molden.input
    if "$ORCA_DIR/orca_2mkl" "$job" -molden > /dev/null && [ -f "$job.molden.input" ]; then
        mv "$job.molden.input" "$job.molden"
        echo "   done: $job.molden, $(ls "$job"-*.cube 2>/dev/null | wc -l | tr -d ' ') cube file(s)"
    else
        echo "   orca_2mkl failed for $job" >&2
        failed+=("$job")
    fi
done

if [ ${#failed[@]} -gt 0 ]; then
    echo "Failed: ${failed[*]}" >&2
    exit 1
fi
echo "All jobs done."
