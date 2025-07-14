#!/bin/bash

set pipefail -eou

if [ $# -lt 2 ]; then
	echo "Usage: $0 <-c|-g>"
	exit 1
fi

proof_mode=$1
counts=("100" "1k" "10k" "100k" "1m" "10m" "100m" "full")
home=$(pwd)

for c in "${counts[@]}"; do
	cd lib/src
	patch="../../patches/patch_${c}.patch"
	patch -p0 < $patch
	cd ../../program; cargo prove build
	cd ../script
	if [ "$proof_mode" == "-g" ]; then
		log_out="../logs/riscv-groth-${c}.log"
		RUST_LOG=error SP1_PROVER=cuda cargo run --release -- --prove --groth > "$log_out"
	else
		log_out="../logs/riscv-compressed-${c}.log"
		RUST_LOG=error SP1_PROVER=cuda cargo run --release -- --prove > "$log_out"
	fi
	cd $home
	git stash push -- lib/
done
