#! /bin/bash -l
#SBATCH -p elephant
#SBATCH -N 1                 # node count
#SBATCH --ntasks-per-node=48
#SBATCH -t 24:00:00          # the six jobs take minutes; generous margin
#SBATCH --mem 200G
#SBATCH -J orca-check

# ORCA orbital check for APOST3Dview: runs the six jobs of this folder one
# after the other, 48 cores each, and writes a .molden file for each
# (see README.md). Submit from this folder: sbatch submit_orca.sh

module load orca/6.1.1

cd "$SLURM_SUBMIT_DIR"
# 200 GB over 48 cores is ~4.2 GB per core; ORCA can exceed %maxcore a
# little, so it gets 3 GB per core.
NPROCS=48 MAXCORE=3000 ./run_all.sh > run_all.log 2>&1
