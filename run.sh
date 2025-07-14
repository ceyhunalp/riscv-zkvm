#!/bin/bash

set -eou pipefail

mkdir -p riscv-zkvm/logs
./run-riscv-zkvm.sh -c
./run-riscv-zkvm.sh -g
