//! P2 SimplicityHL covenant driver (Path A) — C0 anchor + C1 mint-gate.
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

/// Bundled C1 mint-gate program.
pub fn bundled_mint_gate_program() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("programs/mint_gate_covenant.simf")
}

pub fn resolve_mint_gate_program() -> PathBuf {
    let bundled = bundled_mint_gate_program();
    if bundled.is_file() {
        return bundled;
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../programs/simplicity/mint_gate_covenant.simf")
}

/// Byte-reverse a hex asset id (display order → consensus / jet order).
pub fn reverse_hex_bytes(hex_str: &str) -> Result<String> {
    let s = hex_str.trim().trim_start_matches("0x");
    let bytes = hex::decode(s).context("hex decode for reverse")?;
    let rev: Vec<u8> = bytes.into_iter().rev().collect();
    Ok(hex::encode(rev))
}

/// SHA256 of a scriptPubKey (hex) → vault/burn hash param.
/// Empty hex or the token `burn` (C2) hashes the empty script → provably unspendable.
pub fn sha256_spk_hex(spk_hex: &str) -> Result<String> {
    use elements::hashes::{sha256, Hash};
    let t = spk_hex.trim().trim_start_matches("0x");
    let bytes = if t.is_empty() || t.eq_ignore_ascii_case("burn") {
        Vec::new()
    } else {
        hex::decode(t).context("spk hex")?
    };
    Ok(hex::encode(sha256::Hash::hash(&bytes).as_byte_array()))
}

/// SHA256 of the empty script (Elements unspendable / fee-shaped SPK).
/// Used as `VAULT_SPK_HASH` for C2 burn mint-gate (tranche destroyed, not locked).
pub fn empty_spk_hash_hex() -> String {
    use elements::hashes::{sha256, Hash};
    hex::encode(sha256::Hash::hash(&[]).as_byte_array())
}

/// C2 burn target: empty scriptPubKey (provably unspendable).
pub fn burn_spk_bytes() -> Vec<u8> {
    Vec::new()
}

/// Mint-gate `param::` JSON: VAULT_SPK_HASH, BACKING_ASSET (LE), TRANCHE.
pub fn mint_gate_args_json(
    vault_spk_hash_hex: &str,
    backing_asset_le_hex: &str,
    tranche: u64,
) -> Result<String> {
    let v = normalize_hex32(vault_spk_hash_hex)?;
    let a = normalize_hex32(backing_asset_le_hex)?;
    // simplicityhl expects u64 param values as JSON *strings* (not numbers).
    Ok(serde_json::json!({
        "VAULT_SPK_HASH": { "value": format!("0x{v}"), "type": "u256" },
        "BACKING_ASSET": { "value": format!("0x{a}"), "type": "u256" },
        "TRANCHE": { "value": tranche.to_string(), "type": "u64" }
    })
    .to_string())
}

/// Deterministic demo keypair: `sk = SHA256(label)` (regtest only).
pub fn demo_keypair(label: &str) -> Result<(secp256k1::SecretKey, secp256k1::PublicKey)> {
    use elements::hashes::{sha256, Hash};
    let secp = secp256k1::Secp256k1::new();
    let sk_bytes = sha256::Hash::hash(label.as_bytes());
    let sk = secp256k1::SecretKey::from_slice(sk_bytes.as_ref())
        .context("label hashed to an invalid secret key")?;
    let pk = secp256k1::PublicKey::from_secret_key(&secp, &sk);
    Ok((sk, pk))
}

/// P2WPKH scriptPubKey for a compressed pubkey.
pub fn p2wpkh_spk(pk: &secp256k1::PublicKey) -> Vec<u8> {
    use elements::hashes::{hash160, Hash};
    let h = hash160::Hash::hash(&pk.serialize());
    let mut spk = Vec::with_capacity(22);
    spk.push(0x00);
    spk.push(0x14);
    spk.extend_from_slice(h.as_ref());
    spk
}

fn p2wpkh_script_code(pk: &secp256k1::PublicKey) -> Vec<u8> {
    use elements::hashes::{hash160, Hash};
    let h = hash160::Hash::hash(&pk.serialize());
    let mut sc = Vec::with_capacity(25);
    sc.extend_from_slice(&[0x76, 0xa9, 0x14]);
    sc.extend_from_slice(h.as_ref());
    sc.extend_from_slice(&[0x88, 0xac]);
    sc
}

/// Demo P2WPKH address + spk for a label.
pub fn demo_address_info(label: &str) -> Result<serde_json::Value> {
    let (_, pk) = demo_keypair(label)?;
    let btc_pk = elements::bitcoin::PublicKey::new(pk);
    let addr = elements::Address::p2wpkh(&btc_pk, None, &elements::AddressParams::ELEMENTS);
    Ok(serde_json::json!({
        "label": label,
        "address": addr.to_string(),
        "spk_hex": hex::encode(p2wpkh_spk(&pk)),
    }))
}

fn sign_p2wpkh_inputs(
    tx: &mut elements::Transaction,
    inputs: &[(usize, u64)],
    sk: &secp256k1::SecretKey,
    pk: &secp256k1::PublicKey,
) {
    use elements::confidential::Value;
    use elements::hashes::Hash as _;
    use elements::sighash::SighashCache;
    use elements::{EcdsaSighashType, Script};

    let secp = secp256k1::Secp256k1::new();
    let script_code = Script::from(p2wpkh_script_code(pk));
    let mut sigs = Vec::new();
    for &(index, value_sat) in inputs {
        let sighash = SighashCache::new(&*tx).segwitv0_sighash(
            index,
            &script_code,
            Value::Explicit(value_sat),
            EcdsaSighashType::All,
        );
        let msg = secp256k1::Message::from_digest(sighash.to_byte_array());
        let mut sig = secp.sign_ecdsa(&msg, sk).serialize_der().to_vec();
        sig.push(EcdsaSighashType::All as u8);
        sigs.push((index, vec![sig, pk.serialize().to_vec()]));
    }
    for (index, w) in sigs {
        tx.input[index].witness.script_witness = w;
    }
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

/// C1 mint-gate spend request.
#[derive(Debug, Clone)]
pub struct MintSpendRequest {
    pub program_path: PathBuf,
    pub args_json: String,
    /// 32-byte hex MPC/opret root (no 0x required).
    pub anchor_payload_hex: String,
    pub gate_txid: String,
    pub gate_vout: u32,
    pub gate_value_sat: u64,
    pub asset_txid: String,
    pub asset_vout: u32,
    pub fee_txid: String,
    pub fee_vout: u32,
    pub fee_input_sat: u64,
    pub key_label: String,
    pub vault_spk_hex: String,
    /// Display-order asset id (same as `issueasset` / dumpassetlabels style).
    pub backing_asset: String,
    pub tranche: u64,
    pub recipient_spk_hex: String,
    pub recipient_sat: u64,
    pub fee_sat: u64,
    pub lbtc_asset: String,
    pub genesis_hash: String,
    /// `none` | `drop-anchor` | `wrong-amount` | `no-recreate`
    pub tamper: String,
}

/// Build + satisfy + sign a C1 mint-gate spend; returns raw tx hex.
pub fn build_mint_spend(req: &MintSpendRequest) -> Result<String> {
    use elements::confidential::{Asset, Nonce, Value};
    use elements::{
        AssetId, OutPoint, Script, Sequence, Transaction, TxIn, TxInWitness, TxOut, TxOutWitness,
    };

    let src = std::fs::read_to_string(&req.program_path)
        .with_context(|| format!("read {}", req.program_path.display()))?;
    let compiled = compile_src(&src, &req.args_json)?;
    let parts = taproot_parts(&compiled)?;
    let (sk, pk) = demo_keypair(&req.key_label)?;
    let funding_spk = Script::from(p2wpkh_spk(&pk));

    let payload_hex = req.anchor_payload_hex.trim().trim_start_matches("0x");
    let payload = hex::decode(payload_hex).context("anchor payload hex")?;
    anyhow::ensure!(payload.len() == 32, "anchor payload must be 32 bytes");
    let mut opret = Vec::with_capacity(34);
    opret.push(0x6a);
    opret.push(0x20);
    opret.extend_from_slice(&payload);

    let lbtc: AssetId = req.lbtc_asset.parse().context("lbtc asset id")?;
    let backing: AssetId = req.backing_asset.parse().context("backing asset id")?;
    let genesis: elements::BlockHash = req.genesis_hash.parse().context("genesis hash")?;
    let vault_hex = req.vault_spk_hex.trim().trim_start_matches("0x");
    let vault = if vault_hex.is_empty() || vault_hex.eq_ignore_ascii_case("burn") {
        burn_spk_bytes()
    } else {
        hex::decode(vault_hex).context("vault spk hex")?
    };
    let recipient = hex::decode(req.recipient_spk_hex.trim()).context("recipient spk hex")?;

    let change_sat = req
        .fee_input_sat
        .checked_sub(req.recipient_sat + req.fee_sat)
        .context("fee input too small for recipient + fee")?;

    let mk_in = |txid_s: &str, vout: u32| -> Result<TxIn> {
        Ok(TxIn {
            previous_output: OutPoint::new(txid_s.parse()?, vout),
            is_pegin: false,
            script_sig: Script::new(),
            sequence: Sequence::from_consensus(0xffff_fffd),
            asset_issuance: Default::default(),
            witness: TxInWitness::default(),
        })
    };
    let out = |asset: AssetId, sat: u64, spk: Script| TxOut {
        asset: Asset::Explicit(asset),
        value: Value::Explicit(sat),
        nonce: Nonce::Null,
        script_pubkey: spk,
        witness: TxOutWitness::default(),
    };

    let mut tx = Transaction {
        version: 2,
        lock_time: elements::LockTime::ZERO,
        input: vec![
            mk_in(&req.gate_txid, req.gate_vout)?,
            mk_in(&req.asset_txid, req.asset_vout)?,
            mk_in(&req.fee_txid, req.fee_vout)?,
        ],
        output: vec![
            out(lbtc, 0, Script::from(opret)),                     // 0: anchor
            out(backing, req.tranche, Script::from(vault)),        // 1: vault
            out(lbtc, req.recipient_sat, Script::from(recipient)), // 2: recipient seal
            out(lbtc, req.gate_value_sat, parts.spk.clone()),      // 3: next gate
            out(lbtc, change_sat, funding_spk.clone()),            // 4: change
            out(lbtc, req.fee_sat, Script::new()),                 // 5: fee
        ],
    };

    let utxos = vec![
        ElementsUtxo {
            script_pubkey: parts.spk.clone(),
            asset: Asset::Explicit(lbtc),
            value: Value::Explicit(req.gate_value_sat),
        },
        ElementsUtxo {
            script_pubkey: funding_spk.clone(),
            asset: Asset::Explicit(backing),
            value: Value::Explicit(req.tranche),
        },
        ElementsUtxo {
            script_pubkey: funding_spk.clone(),
            asset: Asset::Explicit(lbtc),
            value: Value::Explicit(req.fee_input_sat),
        },
    ];
    let env = ElementsEnv::new(
        Arc::new(tx.clone()),
        utxos,
        0,
        parts.cmr,
        parts.control_block.clone(),
        None,
        genesis,
    );

    let witness_values: WitnessValues = serde_json::from_str(&format!(
        r#"{{ "ANCHOR_PAYLOAD": {{ "value": "0x{payload_hex}", "type": "u256" }} }}"#
    ))
    .context("witness values")?;
    let satisfied = compiled
        .satisfy_with_env(witness_values, Some(&env))
        .map_err(|e| anyhow::anyhow!("satisfy: {e}"))?;
    let (prog_bytes, wit_bytes) = satisfied.redeem().to_vec_with_witness();

    match req.tamper.as_str() {
        "none" => {}
        "drop-anchor" => tx.output[0].script_pubkey = funding_spk.clone(),
        "no-recreate" => tx.output[3].script_pubkey = funding_spk.clone(),
        "wrong-amount" => {
            anyhow::ensure!(req.tranche > 0, "tranche must be > 0 for wrong-amount");
            tx.output[1].value = Value::Explicit(req.tranche - 1);
            tx.output
                .push(out(backing, 1, funding_spk.clone()));
        }
        // C2: covenant expects empty (burn) SPK; pay vault/key instead → consensus reject.
        "not-burn" => {
            tx.output[1].script_pubkey = funding_spk.clone();
        }
        other => bail!("unknown tamper mode: {other} (use none|drop-anchor|wrong-amount|no-recreate|not-burn)"),
    }

    tx.input[0].witness.script_witness = vec![
        wit_bytes,
        prog_bytes,
        parts.leaf_script.clone().into_bytes(),
        parts.control_block.serialize(),
    ];

    sign_p2wpkh_inputs(
        &mut tx,
        &[(1, req.tranche), (2, req.fee_input_sat)],
        &sk,
        &pk,
    );

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

    #[test]
    fn reverse_hex_bytes_roundtrip_length() {
        let a = "b2e15d0d7a0c94e4e2ce0fe6e8691b9e451377f6e46e8045a86f7c4b5d4f0f23";
        let r = reverse_hex_bytes(a).unwrap();
        assert_eq!(r.len(), 64);
        assert_eq!(reverse_hex_bytes(&r).unwrap(), a);
    }

    #[test]
    fn mint_gate_compiles_and_params_affect_cmr() {
        let vault = "aa".repeat(32);
        let asset = "bb".repeat(32);
        let args1 = mint_gate_args_json(&vault, &asset, 250_000).unwrap();
        let args2 = mint_gate_args_json(&vault, &asset, 100_000).unwrap();
        let src = std::fs::read_to_string(bundled_mint_gate_program()).unwrap();
        let c1 = compile_src(&src, &args1).expect("mint gate compiles");
        let c2 = compile_src(&src, &args2).expect("mint gate compiles");
        assert_eq!(c1.commit().cmr(), compile_src(&src, &args1).unwrap().commit().cmr());
        assert_ne!(c1.commit().cmr(), c2.commit().cmr());
        let parts = taproot_parts(&c1).unwrap();
        assert_eq!(parts.control_block.leaf_version.as_u8(), 0xbe);
        assert!(parts.address.to_string().starts_with("ert1p"));
    }

    #[test]
    fn c2_burn_empty_spk_hash_is_stable() {
        let h = empty_spk_hash_hex();
        assert_eq!(h.len(), 64);
        assert_eq!(h, sha256_spk_hex("").unwrap());
        assert_eq!(h, sha256_spk_hex("burn").unwrap());
        // Known SHA256("")
        assert_eq!(
            h,
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        let asset = "cc".repeat(32);
        let args_lock = mint_gate_args_json(&"aa".repeat(32), &asset, 250_000).unwrap();
        let args_burn = mint_gate_args_json(&h, &asset, 250_000).unwrap();
        let src = std::fs::read_to_string(bundled_mint_gate_program()).unwrap();
        let lock = compile_src(&src, &args_lock).unwrap();
        let burn = compile_src(&src, &args_burn).unwrap();
        // Different burn vs vault targets → different gate addresses.
        assert_ne!(lock.commit().cmr(), burn.commit().cmr());
    }
}
