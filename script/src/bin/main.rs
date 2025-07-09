use std::time::{Duration, Instant};

use alloy_sol_types::SolType;
use clap::Parser;
use riscv_zkvm_lib::PublicValuesStruct;
use sp1_sdk::{include_elf, ProverClient, SP1ProofWithPublicValues, SP1Stdin};

/// The ELF (executable and linkable format) file for the Succinct RISC-V zkVM.
pub const RISCV_ZKVM_ELF: &[u8] = include_elf!("riscv-zkvm-program");

/// The arguments for the command.
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(long)]
    execute: bool,

    #[arg(long)]
    prove: bool,

    #[arg(long)]
    groth: bool,

    #[arg(long, default_value = "1")]
    runs: u32,
}

fn main() {
    // Setup the logger.
    sp1_sdk::utils::setup_logger();
    dotenv::dotenv().ok();

    // Parse the command line arguments.
    let args = Args::parse();

    if args.execute == args.prove {
        eprintln!("Error: You must specify either --execute or --prove");
        std::process::exit(1);
    }

    // Setup the prover client.
    let client = ProverClient::from_env();

    // Setup the inputs.
    let mut stdin = SP1Stdin::new();

    let blob_bytes = include_bytes!("../../../blobs/guest-program.bin");
    let calldata = blob_bytes.to_vec();
    stdin.write(&calldata);

    if args.execute {
        // Execute the program
        let (output, report) = client.execute(RISCV_ZKVM_ELF, &stdin).run().unwrap();
        println!("Program executed successfully.");

        // Read the output.
        let decoded = PublicValuesStruct::abi_decode(output.as_slice()).unwrap();
        let PublicValuesStruct { count } = decoded;

        println!("Count is: {count}");

        // println!("Instruction count: {}", report.total_instruction_count());
    } else {
        // Setup the program for proving.
        let (pk, vk) = client.setup(RISCV_ZKVM_ELF);

        let mut elapsed_vec: Vec<Duration> = Vec::new();
        for _ in 0..args.runs {
            // Generate the proof
            let mut proof: SP1ProofWithPublicValues;
            let proving_duration: Duration;
            if args.groth {
                let proof_start = Instant::now();
                proof = client
                    .prove(&pk, &stdin)
                    .groth16()
                    .run()
                    .expect("failed to generate proof");
                proving_duration = proof_start.elapsed();
                elapsed_vec.push(proving_duration);
            } else {
                let proof_start = Instant::now();
                proof = client
                    .prove(&pk, &stdin)
                    .compressed()
                    .run()
                    .expect("failed to generate proof");
                proving_duration = proof_start.elapsed();
                elapsed_vec.push(proving_duration);
            }
            println!("Successfully generated proof!");

            let count = proof.public_values.read::<u128>();
            println!("Frame count is: {count}");

            // Verify the proof
            client.verify(&proof, &vk).expect("failed to verify proof");
            println!("Successfully verified proof!");
        }

        let mut total_time = 0.0;
        for i in 0..args.runs as usize {
            println!("{}: {}", i, elapsed_vec[i].as_secs_f64());
            total_time += elapsed_vec[i].as_secs_f64();
        }
        let avg_time = total_time / (args.runs as f64);
        println!("Average time: {avg_time}");
    }
}
