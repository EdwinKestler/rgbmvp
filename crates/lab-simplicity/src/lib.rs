//! P2 C0 — SimplicityHL RGB-anchor covenant driver (Path A).
//!
//! Adapted from kaleidoswap/rgb-on-liquid-spike `spike-simplicity` (MIT OR Apache-2.0).
//! Pins: simplicityhl 0.6 · simplicity-lang 0.8 · tapleaf 0xbe · opret-shaped vout0.
//! See `docs/P2_SIMPLICITY.md`.

use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::Arc;

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use simplicity::jet::elements::{ElementsEnv, ElementsUtxo};
use simplicityhl::ast::ElementsJetHinter;
use simplicityhl::elements;
use simplicityhl::{Arguments, CompiledProgram, TemplateProgram, WitnessValues};

use elements::secp256k1_zkp as secp256k1;

/// BIP-341 NUMS point — no known discrete log (key-path disabled).
pub const NUMS_KEY: &str = "50929b74c1a04954b78b4b6035e97a5e078a5a0f28ec96d547bfee9ace803ac0";

/// Bundled C0 program (relative to this crate).
pub fn bundled_rgb_anchor_program() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("programs/rgb_anchor_covenant.simf")
}

/// Also published at repo root for operators.
pub fn repo_rgb_anchor_program() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../programs/simplicity/rgb_anchor_covenant.simf")
}

/// Resolve the C0 program path (crate copy preferred, then repo root).
pub fn resolve_rgb_anchor_program() -> PathBuf {
    let bundled = bundled_rgb_anchor_program();
    if bundled.is_file() {
        return bundled;
    }
    repo_rgb_anchor_program()
}

/// RGB `opret` shape: `OP_RETURN OP_PUSHBYTES_32 <payload>`.
pub fn opret_spk(payload: &[u8; 32]) -> Vec<u8> {
    let mut spk = Vec::with_capacity(34);
    spk.push(0x6a);
    spk.push(0x20);
    spk.extend_from_slice(payload);
    spk
}

pub fn compile_file(program_path: &Path, args_path: &Path) -> Result<CompiledProgram> {
    let src = std::fs::read_to_string(program_path)
        .with_context(|| format!("read {}", program_path.display()))?;
    let args_json = std::fs::read_to_string(args_path)
        .with_context(|| format!("read {}", args_path.display()))?;
    compile_src(&src, &args_json)
}

pub fn compile_src(src: &str, args_json: &str) -> Result<CompiledProgram> {
    let template = TemplateProgram::new(src, Box::new(ElementsJetHinter::new()))
        .map_err(|e| anyhow::anyhow!("parse: {e}"))?;
    let args: Arguments = serde_json::from_str(args_json).context("parse args JSON")?;
    template
        .instantiate(args, false)
        .map_err(|e| anyhow::anyhow!("instantiate: {e}"))
}

/// JSON for `param::EXPECTED_HASH` (u256 hex with 0x prefix).
pub fn args_expected_hash_json(hash_hex: &str) -> Result<String> {
    let h = normalize_hex32(hash_hex)?;
    Ok(serde_json::json!({
        "EXPECTED_HASH": { "value": format!("0x{h}"), "type": "u256" }
    })
    .to_string())
}

/// JSON for witness PREIMAGE + ANCHOR_PAYLOAD.
pub fn witness_json(preimage_hex: &str, anchor_payload_hex: &str) -> Result<String> {
    let p = normalize_hex32(preimage_hex)?;
    let a = normalize_hex32(anchor_payload_hex)?;
    Ok(serde_json::json!({
        "PREIMAGE": { "value": format!("0x{p}"), "type": "u256" },
        "ANCHOR_PAYLOAD": { "value": format!("0x{a}"), "type": "u256" }
    })
    .to_string())
}

fn normalize_hex32(s: &str) -> Result<String> {
    let s = s.trim().trim_start_matches("0x");
    let bytes = hex::decode(s).context("hex decode")?;
    if bytes.len() != 32 {
        bail!("expected 32 bytes, got {}", bytes.len());
    }
    Ok(hex::encode(bytes))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CovenantAddressInfo {
    pub cmr: String,
    pub address: String,
    pub spk_hex: String,
    pub leaf_version: String,
}

pub struct TaprootParts {
    pub address: elements::Address,
    pub spk: elements::Script,
    pub leaf_script: elements::Script,
    pub control_block: elements::taproot::ControlBlock,
    pub cmr: simplicity::Cmr,
}

pub fn taproot_parts(compiled: &CompiledProgram) -> Result<TaprootParts> {
    let secp = secp256k1::Secp256k1::new();
    let cmr = compiled.commit().cmr();
    let leaf_script = elements::Script::from(cmr.as_ref().to_vec());
    let internal_key = elements::bitcoin::key::XOnlyPublicKey::from_str(NUMS_KEY)?;

    let spend_info = elements::taproot::TaprootBuilder::new()
        .add_leaf_with_ver(0, leaf_script.clone(), simplicity::leaf_version())
        .map_err(|e| anyhow::anyhow!("taproot builder: {e:?}"))?
        .finalize(&secp, internal_key)
        .map_err(|e| anyhow::anyhow!("taproot finalize: {e:?}"))?;

    let control_block = spend_info
        .control_block(&(leaf_script.clone(), simplicity::leaf_version()))
        .context("control block for simplicity leaf")?;

    let address = elements::Address::p2tr(
        &secp,
        internal_key,
        spend_info.merkle_root(),
        None,
        &elements::AddressParams::ELEMENTS,
    );
    let spk = address.script_pubkey();

    Ok(TaprootParts {
        address,
        spk,
        leaf_script,
        control_block,
        cmr,
    })
}

pub fn address_info(compiled: &CompiledProgram) -> Result<CovenantAddressInfo> {
    let parts = taproot_parts(compiled)?;
    Ok(CovenantAddressInfo {
        cmr: parts.cmr.to_string(),
        address: parts.address.to_string(),
        spk_hex: hex::encode(parts.spk.as_bytes()),
        leaf_version: "0xbe".into(),
    })
}

#[derive(Debug, Clone)]
pub struct SpendRequest {
    pub program_path: PathBuf,
    pub args_json: String,
    pub witness_json: String,
    pub prev_txid: String,
    pub prev_vout: u32,
    pub input_value_sat: u64,
    pub dest_spk_hex: String,
    pub fee_sat: u64,
    pub lbtc_asset: String,
    pub genesis_hash: String,
    /// 32-byte hex for opret payload at vout 0 (required for C0).
    pub opret_payload_hex: String,
    /// After satisfy, strip opret (consensus-negative test).
    pub tamper_drop_anchor: bool,
}

/// Build + satisfy a C0 spend; returns raw Elements tx hex.
pub fn build_spend(req: &SpendRequest) -> Result<String> {
    use elements::confidential::{Asset, Nonce, Value};
    use elements::{
        AssetId, OutPoint, Script, Sequence, Transaction, TxIn, TxInWitness, TxOut, TxOutWitness,
    };

    let src = std::fs::read_to_string(&req.program_path)
        .with_context(|| format!("read {}", req.program_path.display()))?;
    let compiled = compile_src(&src, &req.args_json)?;
    let parts = taproot_parts(&compiled)?;

    let witness_values: WitnessValues =
        serde_json::from_str(&req.witness_json).context("parse witness JSON")?;

    let txid: elements::Txid = req.prev_txid.parse().context("prev_txid")?;
    let asset_id: AssetId = req.lbtc_asset.parse().context("lbtc asset id")?;
    let lbtc = Asset::Explicit(asset_id);
    let genesis: elements::BlockHash = req.genesis_hash.parse().context("genesis hash")?;
    let dest = hex::decode(&req.dest_spk_hex).context("dest_spk hex")?;

    let payload: [u8; 32] = hex::decode(req.opret_payload_hex.trim().trim_start_matches("0x"))
        .context("opret payload hex")?
        .try_into()
        .map_err(|_| anyhow::anyhow!("opret payload must be 32 bytes"))?;
    let opret = opret_spk(&payload);

    let output = vec![
        TxOut {
            asset: lbtc,
            value: Value::Explicit(0),
            nonce: Nonce::Null,
            script_pubkey: Script::from(opret),
            witness: TxOutWitness::default(),
        },
        TxOut {
            asset: lbtc,
            value: Value::Explicit(
                req.input_value_sat
                    .checked_sub(req.fee_sat)
                    .context("fee exceeds input")?,
            ),
            nonce: Nonce::Null,
            script_pubkey: Script::from(dest),
            witness: TxOutWitness::default(),
        },
        TxOut {
            asset: lbtc,
            value: Value::Explicit(req.fee_sat),
            nonce: Nonce::Null,
            script_pubkey: Script::new(),
            witness: TxOutWitness::default(),
        },
    ];

    let mut tx = Transaction {
        version: 2,
        lock_time: elements::LockTime::ZERO,
        input: vec![TxIn {
            previous_output: OutPoint::new(txid, req.prev_vout),
            is_pegin: false,
            script_sig: Script::new(),
            sequence: Sequence::from_consensus(0xffff_fffd),
            asset_issuance: Default::default(),
            witness: TxInWitness::default(),
        }],
        output,
    };

    let utxo = ElementsUtxo {
        script_pubkey: parts.spk.clone(),
        asset: lbtc,
        value: Value::Explicit(req.input_value_sat),
    };
    let env = ElementsEnv::new(
        Arc::new(tx.clone()),
        vec![utxo],
        0,
        parts.cmr,
        parts.control_block.clone(),
        None,
        genesis,
    );

    let satisfied = compiled
        .satisfy_with_env(witness_values, Some(&env))
        .map_err(|e| anyhow::anyhow!("satisfy: {e}"))?;
    let (prog_bytes, wit_bytes) = satisfied.redeem().to_vec_with_witness();

    if req.tamper_drop_anchor {
        tx.output.remove(0);
    }

    tx.input[0].witness.script_witness = vec![
        wit_bytes,
        prog_bytes,
        parts.leaf_script.clone().into_bytes(),
        parts.control_block.serialize(),
    ];

    Ok(hex::encode(elements::encode::serialize(&tx)))
}

#[cfg(test)]
mod tests {
    use super::*;

    const HASH_A: &str = "66687aadf862bd776c8fc18b8e9f8e20089714856ee233b3902a591d0d5f2925";
    const HASH_B: &str = "0000000000000000000000000000000000000000000000000000000000000001";

    fn compile_with_hash(hash: &str) -> CompiledProgram {
        let args = args_expected_hash_json(hash).unwrap();
        let src = std::fs::read_to_string(bundled_rgb_anchor_program()).unwrap();
        compile_src(&src, &args).expect("covenant program compiles")
    }

    #[test]
    fn opret_spk_is_anchor_shaped() {
        let payload = [0xabu8; 32];
        let spk = opret_spk(&payload);
        assert_eq!(spk.len(), 34);
        assert_eq!(spk[0], 0x6a);
        assert_eq!(spk[1], 0x20);
        assert_eq!(&spk[2..], &payload);
    }

    #[test]
    fn bundled_covenant_compiles_and_cmr_is_deterministic() {
        let a1 = compile_with_hash(HASH_A);
        let a2 = compile_with_hash(HASH_A);
        assert_eq!(a1.commit().cmr(), a2.commit().cmr());
    }

    #[test]
    fn hash_argument_is_baked_into_the_cmr() {
        let a = compile_with_hash(HASH_A);
        let b = compile_with_hash(HASH_B);
        assert_ne!(a.commit().cmr(), b.commit().cmr());
    }

    #[test]
    fn taproot_parts_are_wellformed() {
        let parts = taproot_parts(&compile_with_hash(HASH_A)).unwrap();
        assert_eq!(parts.leaf_script.as_bytes(), parts.cmr.as_ref());
        let spk = parts.spk.as_bytes();
        assert_eq!(spk.len(), 34);
        assert_eq!(spk[0], 0x51);
        assert_eq!(spk[1], 0x20);
        assert_eq!(parts.control_block.serialize().len(), 33);
        assert_eq!(parts.control_block.leaf_version.as_u8(), 0xbe);
        assert!(parts.address.to_string().starts_with("ert1p"));
    }

    #[test]
    fn addresses_differ_per_hashlock() {
        let a = taproot_parts(&compile_with_hash(HASH_A)).unwrap();
        let b = taproot_parts(&compile_with_hash(HASH_B)).unwrap();
        assert_ne!(a.address.to_string(), b.address.to_string());
    }
}
