%nproc=8
%mem=16GB
%chk=H2O-MP2-ccpVTZ.chk
# MP2/cc-pVTZ density=current pop=full gfinput

H2O, MP2 relaxed density

0 1
8       -0.772305631      0.000000000     -1.921100136
1       -0.772305631      0.759337000     -1.325057136
1       -0.772305631     -0.759337000     -1.325057136

