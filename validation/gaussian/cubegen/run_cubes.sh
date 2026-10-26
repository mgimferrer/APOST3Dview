#! /bin/bash -l
#SBATCH -J rev1-cubegen
#SBATCH -p elephant
#SBATCH -N 1
#SBATCH --ntasks-per-node=8
#SBATCH -t 12:00:00
#SBATCH --mem 20G

# Gaussian's own cube files (cubegen) for every line of cubes.txt, 80
# points per side, with header. Run after run_new.sh has finished
# (sbatch --dependency=afterok:<jobid> run_cubes.sh). Submit from 1-gaussian/.
module load g16_a03
cd "$SLURM_SUBMIT_DIR"
mkdir -p cubes
grep -v '^#' cubes.txt | while read -r fchk kind cube; do
    [ -z "$fchk" ] && continue
    if [ ! -f "fchk/$fchk.fchk" ]; then
        echo "missing fchk/$fchk.fchk (for $cube)"; continue
    fi
    cubegen 8 "$kind" "fchk/$fchk.fchk" "cubes/$cube.cube" 80 h || echo "cubegen failed: $cube"
done
echo "$(ls cubes/*.cube | wc -l) cube files written; $(date)"
