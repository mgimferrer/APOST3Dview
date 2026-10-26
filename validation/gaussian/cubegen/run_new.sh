#! /bin/bash -l
#SBATCH -J rev1-g16
#SBATCH -p elephant
#SBATCH -N 1
#SBATCH --ntasks-per-node=8
#SBATCH -t 24:00:00
#SBATCH --mem 20G

# The four new Gaussian jobs of new/ (small single points), each followed
# by formchk into ../fchk/. Submit from 1-gaussian/: sbatch run_new.sh
module load g16_a03
cd "$SLURM_SUBMIT_DIR/new"
for com in *.com; do
    job=${com%.com}
    echo "== $job $(date)"
    g16 "$com"
    if grep -q "Normal termination" "$job.log"; then
        formchk "$job.chk" "../fchk/$job.fchk"
    else
        echo "   $job did not end normally, see new/$job.log"
    fi
done
echo "Finished $(date)"
