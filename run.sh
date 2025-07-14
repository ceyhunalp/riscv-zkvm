#!/bin/bash

set pipefail -eou

mkdir -p riscv-zkvm/logs
./run-riscv-zkvm.sh -c
./run-riscv-zkvm.sh -g
