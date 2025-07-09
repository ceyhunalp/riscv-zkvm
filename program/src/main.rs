#![no_main]
sp1_zkvm::entrypoint!(main);

use alloy_sol_types::SolType;
use riscv_zkvm_lib::{riscv, PublicValuesStruct};

pub fn main() {
    let calldata = sp1_zkvm::io::read::<Vec<u8>>();
    let count = riscv(&calldata);
    sp1_zkvm::io::commit(&count);

    // let bytes = PublicValuesStruct::abi_encode(&PublicValuesStruct { count });
    // sp1_zkvm::io::commit_slice(&bytes);
}
