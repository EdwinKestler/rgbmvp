//! `lab-simp` — drive C0 RGB-anchor Simplicity covenant (address + spend).

use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use lab_simplicity::{
    address_info, args_expected_hash_json, build_spend, compile_src, resolve_rgb_anchor_program,
    witness_json, SpendRequest,
};

#[derive(Parser, Debug)]
#[command(
    name = "lab-simp",
    about = "P2 C0: Simplicity RGB-anchor covenant (Path A)"
)]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand, Debug)]
enum Cmd {
    /// Compile program + print taproot address (leaf 0xbe).
    Address {
        #[arg(long)]
        program: Option<PathBuf>,
        /// Path to args JSON, or use --hash for EXPECTED_HASH helper.
        #[arg(long)]
        args: Option<PathBuf>,
        /// 32-byte hex expected SHA256(preimage) → builds args JSON.
        #[arg(long)]
        hash: Option<String>,
    },
    /// Build + satisfy spend tx; print raw hex.
    Spend {
        #[arg(long)]
        program: Option<PathBuf>,
        #[arg(long)]
        args: Option<PathBuf>,
        #[arg(long)]
        hash: Option<String>,
        #[arg(long)]
        witness: Option<PathBuf>,
        #[arg(long)]
        preimage: Option<String>,
        #[arg(long)]
        anchor_payload: Option<String>,
        #[arg(long)]
        prev_txid: String,
        #[arg(long)]
        prev_vout: u32,
        #[arg(long)]
        input_value_sat: u64,
        #[arg(long)]
        dest_spk: String,
        #[arg(long, default_value_t = 1000)]
        fee_sat: u64,
        #[arg(long)]
        lbtc_asset: String,
        #[arg(long)]
        genesis_hash: String,
        #[arg(long)]
        opret_payload: String,
        #[arg(long, default_value_t = false)]
        tamper_drop_anchor: bool,
    },
}

fn load_args(args: Option<PathBuf>, hash: Option<String>) -> Result<String> {
    if let Some(p) = args {
        return std::fs::read_to_string(p).context("read args file");
    }
    if let Some(h) = hash {
        return args_expected_hash_json(&h);
    }
    anyhow::bail!("provide --args <file> or --hash <hex32>");
}

fn load_witness(
    witness: Option<PathBuf>,
    preimage: Option<String>,
    anchor_payload: Option<String>,
    opret_payload: &str,
) -> Result<String> {
    if let Some(p) = witness {
        return std::fs::read_to_string(p).context("read witness file");
    }
    let pre = preimage.context("provide --witness <file> or --preimage")?;
    let anc = anchor_payload.unwrap_or_else(|| opret_payload.to_string());
    witness_json(&pre, &anc)
}

fn main() -> Result<()> {
    match Cli::parse().cmd {
        Cmd::Address {
            program,
            args,
            hash,
        } => {
            let program = program.unwrap_or_else(resolve_rgb_anchor_program);
            let args_json = load_args(args, hash)?;
            let src = std::fs::read_to_string(&program)
                .with_context(|| format!("read {}", program.display()))?;
            let compiled = compile_src(&src, &args_json)?;
            let info = address_info(&compiled)?;
            println!("{}", serde_json::to_string_pretty(&info)?);
            Ok(())
        }
        Cmd::Spend {
            program,
            args,
            hash,
            witness,
            preimage,
            anchor_payload,
            prev_txid,
            prev_vout,
            input_value_sat,
            dest_spk,
            fee_sat,
            lbtc_asset,
            genesis_hash,
            opret_payload,
            tamper_drop_anchor,
        } => {
            let program = program.unwrap_or_else(resolve_rgb_anchor_program);
            let args_json = load_args(args, hash)?;
            let witness_json =
                load_witness(witness, preimage, anchor_payload, &opret_payload)?;
            let hex_tx = build_spend(&SpendRequest {
                program_path: program,
                args_json,
                witness_json,
                prev_txid,
                prev_vout,
                input_value_sat,
                dest_spk_hex: dest_spk,
                fee_sat,
                lbtc_asset,
                genesis_hash,
                opret_payload_hex: opret_payload,
                tamper_drop_anchor,
            })?;
            println!("{hex_tx}");
            Ok(())
        }
    }
}
