//! `lab-simp` — drive C0 RGB-anchor + C1/C2 mint-gate Simplicity covenants.

use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use lab_simplicity::{
    address_info, args_expected_hash_json, build_mint_spend, build_spend, compile_src,
    demo_address_info, empty_spk_hash_hex, mint_gate_args_json, resolve_mint_gate_program,
    resolve_rgb_anchor_program, reverse_hex_bytes, sha256_spk_hex, witness_json, MintSpendRequest,
    SpendRequest,
};

#[derive(Parser, Debug)]
#[command(
    name = "lab-simp",
    about = "P2 Simplicity covenants: C0 anchor + C1 vault / C2 burn mint-gate (Path A)"
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
        #[arg(long)]
        args: Option<PathBuf>,
        /// C0: 32-byte hex EXPECTED_HASH.
        #[arg(long)]
        hash: Option<String>,
        /// C1: vault scriptPubKey hex → VAULT_SPK_HASH.
        #[arg(long)]
        vault_spk: Option<String>,
        /// C2 burn: bake empty-script hash as VAULT_SPK_HASH (provably unspendable burn).
        #[arg(long, default_value_t = false)]
        burn: bool,
        /// C1/C2: backing asset id (display hex; byte-reversed into args).
        #[arg(long)]
        backing_asset: Option<String>,
        /// C1/C2: tranche (asset sats locked or burned per mint).
        #[arg(long)]
        tranche: Option<u64>,
    },
    /// Build + satisfy C0 spend; print raw hex.
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
    /// Demo P2WPKH address for a label (minter funding key).
    DemoAddress {
        #[arg(long, default_value = "minter")]
        label: String,
    },
    /// Build + satisfy + sign C1 mint-gate spend; print raw hex.
    MintSpend {
        #[arg(long)]
        program: Option<PathBuf>,
        #[arg(long)]
        args: Option<PathBuf>,
        #[arg(long)]
        vault_spk: Option<String>,
        #[arg(long)]
        backing_asset: Option<String>,
        #[arg(long)]
        tranche: Option<u64>,
        #[arg(long)]
        anchor_payload: String,
        #[arg(long)]
        gate_txid: String,
        #[arg(long)]
        gate_vout: u32,
        #[arg(long)]
        gate_value_sat: u64,
        #[arg(long)]
        asset_txid: String,
        #[arg(long)]
        asset_vout: u32,
        #[arg(long)]
        fee_txid: String,
        #[arg(long)]
        fee_vout: u32,
        #[arg(long)]
        fee_input_sat: u64,
        #[arg(long, default_value = "minter")]
        key_label: String,
        #[arg(long)]
        vault_spk_out: Option<String>,
        /// C2: burn TRANCHE of BACKING_ASSET to empty SPK (not vault lock).
        #[arg(long, default_value_t = false)]
        burn: bool,
        #[arg(long)]
        recipient_spk: String,
        #[arg(long, default_value_t = 5_000)]
        recipient_sat: u64,
        #[arg(long, default_value_t = 2_000)]
        fee_sat: u64,
        #[arg(long)]
        lbtc_asset: String,
        #[arg(long)]
        genesis_hash: String,
        /// none | drop-anchor | wrong-amount | no-recreate | not-burn (C2)
        #[arg(long, default_value = "none")]
        tamper: String,
    },
}

fn load_c0_args(args: Option<PathBuf>, hash: Option<String>) -> Result<String> {
    if let Some(p) = args {
        return std::fs::read_to_string(p).context("read args file");
    }
    if let Some(h) = hash {
        return args_expected_hash_json(&h);
    }
    anyhow::bail!("provide --args, or --hash (C0), or mint-gate flags");
}

fn load_mint_args(
    args: Option<PathBuf>,
    vault_spk: Option<String>,
    backing_asset: Option<String>,
    tranche: Option<u64>,
    burn: bool,
) -> Result<String> {
    if let Some(p) = args {
        return std::fs::read_to_string(p).context("read args file");
    }
    let asset = backing_asset.context("provide --backing-asset")?;
    let t = tranche.context("provide --tranche")?;
    let vault_hash = if burn {
        empty_spk_hash_hex()
    } else {
        let vault =
            vault_spk.context("provide --args or --vault-spk + --backing-asset + --tranche")?;
        sha256_spk_hex(&vault)?
    };
    let asset_le = reverse_hex_bytes(&asset)?;
    mint_gate_args_json(&vault_hash, &asset_le, t)
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
            vault_spk,
            burn,
            backing_asset,
            tranche,
        } => {
            let is_mint =
                burn || vault_spk.is_some() || backing_asset.is_some() || tranche.is_some();
            let program = program.unwrap_or_else(|| {
                if is_mint {
                    resolve_mint_gate_program()
                } else {
                    resolve_rgb_anchor_program()
                }
            });
            let args_json = if is_mint || (args.is_some() && hash.is_none()) {
                // Prefer mint path when mint flags set; else if only --args, compile as-is
                // with default program still C0 unless program points to mint gate.
                if is_mint {
                    load_mint_args(args, vault_spk, backing_asset, tranche, burn)?
                } else if let Some(p) = args {
                    std::fs::read_to_string(p)?
                } else {
                    load_c0_args(None, hash)?
                }
            } else {
                load_c0_args(args, hash)?
            };
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
            let args_json = load_c0_args(args, hash)?;
            let witness_json = load_witness(witness, preimage, anchor_payload, &opret_payload)?;
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
        Cmd::DemoAddress { label } => {
            println!("{}", serde_json::to_string_pretty(&demo_address_info(&label)?)?);
            Ok(())
        }
        Cmd::MintSpend {
            program,
            args,
            vault_spk,
            backing_asset,
            tranche,
            anchor_payload,
            gate_txid,
            gate_vout,
            gate_value_sat,
            asset_txid,
            asset_vout,
            fee_txid,
            fee_vout,
            fee_input_sat,
            key_label,
            vault_spk_out,
            burn,
            recipient_spk,
            recipient_sat,
            fee_sat,
            lbtc_asset,
            genesis_hash,
            tamper,
        } => {
            let program = program.unwrap_or_else(resolve_mint_gate_program);
            let vault_out = if burn {
                String::new()
            } else {
                vault_spk_out
                    .clone()
                    .or_else(|| vault_spk.clone())
                    .context("provide --vault-spk-out or --vault-spk for vault output (or --burn)")?
            };
            let backing = backing_asset
                .clone()
                .context("provide --backing-asset")?;
            let tranche_v = tranche.context("provide --tranche")?;
            let vault_for_args = if burn {
                Some(String::new())
            } else {
                vault_spk.or(vault_spk_out)
            };
            let args_json =
                load_mint_args(args, vault_for_args, Some(backing.clone()), Some(tranche_v), burn)?;
            let hex_tx = build_mint_spend(&MintSpendRequest {
                program_path: program,
                args_json,
                anchor_payload_hex: anchor_payload,
                gate_txid,
                gate_vout,
                gate_value_sat,
                asset_txid,
                asset_vout,
                fee_txid,
                fee_vout,
                fee_input_sat,
                key_label,
                vault_spk_hex: vault_out,
                backing_asset: backing,
                tranche: tranche_v,
                recipient_spk_hex: recipient_spk,
                recipient_sat,
                fee_sat,
                lbtc_asset,
                genesis_hash,
                tamper,
            })?;
            println!("{hex_tx}");
            Ok(())
        }
    }
}
