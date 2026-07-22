//! RGB-on-Liquid (P0): NIA issue/transfer, MPC+tapret anchor, WitnessTx verify.
//!
//! Adapted from kaleidoswap/rgb-on-liquid-spike against vendored `rgb-consensus`
//! with the `WitnessTx` patch.

pub mod bfa;
pub mod htlc;
pub mod liquid_dbc;
pub mod mint;
pub mod mpc;
pub mod patched_anchor;
pub mod rgb20;
pub mod seal;
pub mod storage;
pub mod swap;
pub mod tapret_addr;

use anyhow::{Context, Result};
use rgbcore::bitcoin::hashes::Hash;
use rgbcore::bitcoin::{OutPoint, Txid};
use rgbcore::commit_verify::CommitId;
use rgbcore::{ChainNet, ContractId};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Fixed genesis timestamp for deterministic contract ids (spike BFA constant).
pub const GENESIS_TIMESTAMP: i64 = 1_735_689_600;

/// Demo internal x-only key for naked tapret (not a real spend key).
pub const DEMO_INTERNAL_XONLY_HEX: &str =
    "d6889cb081036e0faefa3a35157ad71086b123b2b144b649798b494c300a961d";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IssueRequest {
    pub name: String,
    pub ticker: String,
    pub supply: u64,
    pub seal: String,
    /// liquid-testnet | bitcoin-testnet | bitcoin-testnet3 | …
    #[serde(default = "default_liquid_chain")]
    pub chain: String,
}

fn default_liquid_chain() -> String {
    "liquid-testnet".into()
}

pub fn parse_chain_net(s: &str) -> Result<ChainNet> {
    match s.trim().to_ascii_lowercase().as_str() {
        "liquid-testnet" | "liquid_testnet" | "tl" | "elements-regtest" | "liquid-regtest" => {
            // RGB genesis stamp: Elements regtest demos use LiquidTestnet id.
            Ok(ChainNet::LiquidTestnet)
        }
        "bitcoin-testnet" | "bitcoin-testnet3" | "testnet" | "testnet3" | "tb" | "tb3" => {
            Ok(ChainNet::BitcoinTestnet3)
        }
        "bitcoin-testnet4" | "testnet4" | "tb4" => Ok(ChainNet::BitcoinTestnet4),
        "bitcoin-regtest" | "regtest" | "bcrt" => Ok(ChainNet::BitcoinRegtest),
        other => anyhow::bail!("unsupported chain {other:?}"),
    }
}

pub fn chain_label(c: ChainNet) -> &'static str {
    match c {
        ChainNet::LiquidTestnet => "liquid-testnet",
        ChainNet::BitcoinTestnet3 => "bitcoin-testnet",
        ChainNet::BitcoinTestnet4 => "bitcoin-testnet4",
        ChainNet::BitcoinRegtest => "bitcoin-regtest",
        _ => "unknown",
    }
}

pub fn tapret_hrp(c: ChainNet) -> &'static str {
    match c {
        ChainNet::LiquidTestnet => tapret_addr::HRP_TESTNET,
        ChainNet::LiquidMainnet => tapret_addr::HRP_LIQUIDV1,
        ChainNet::BitcoinTestnet3 | ChainNet::BitcoinTestnet4 => "tb",
        ChainNet::BitcoinRegtest => "bcrt",
        ChainNet::BitcoinMainnet => "bc",
        _ => "tb",
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IssueResult {
    pub contract_id: String,
    pub chain_net: String,
    pub ticker: String,
    pub name: String,
    pub supply: u64,
    pub seal: String,
    pub genesis_opid_hex: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransferPlan {
    pub contract_id: String,
    #[serde(default = "default_liquid_chain")]
    pub chain_net: String,
    pub ticker: String,
    pub send_amount: u64,
    pub change_amount: u64,
    pub alice_seal: String,
    pub bob_seal_placeholder: String,
    pub change_seal_placeholder: String,
    pub bundle_id_hex: String,
    pub transition_opid_hex: String,
    pub mpc_root_hex: String,
    pub commitment_spk_hex: String,
    pub tapret_address: String,
    pub internal_key_hex: String,
    pub static_entropy: u64,
    pub protocol_id_hex: String,
    pub message_hex: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerifyResult {
    pub status: String,
    pub contract_id: Option<String>,
    pub anchor_txid: String,
    pub checks: Vec<VerifyCheck>,
    pub explorer_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerifyCheck {
    pub name: String,
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

pub fn parse_outpoint(s: &str) -> Result<OutPoint> {
    let (txid_s, vout_s) = s
        .split_once(':')
        .or_else(|| s.split_once('/'))
        .context("outpoint must be txid:vout")?;
    let txid = Txid::from_byte_array(parse32_hex(txid_s, "txid")?);
    let vout: u32 = vout_s.parse().context("vout")?;
    Ok(OutPoint::new(txid, vout))
}

pub fn parse32_hex(s: &str, label: &str) -> Result<[u8; 32]> {
    let b = hex::decode(s.trim()).with_context(|| format!("{label} hex"))?;
    if b.len() != 32 {
        anyhow::bail!("{label} must be 32 bytes, got {}", b.len());
    }
    let mut out = [0u8; 32];
    out.copy_from_slice(&b);
    Ok(out)
}

pub fn parse_contract_id(s: &str) -> Result<ContractId> {
    let t = s.trim();
    if t.len() == 64 && t.chars().all(|c| c.is_ascii_hexdigit()) {
        let b = parse32_hex(t, "contract_id")?;
        return Ok(ContractId::from(b));
    }
    t.parse::<ContractId>()
        .map_err(|e| anyhow::anyhow!("parse contract id {t:?}: {e}"))
}

pub fn issue_nia(req: &IssueRequest) -> Result<IssueResult> {
    let seal = parse_outpoint(&req.seal)?;
    let chain = parse_chain_net(&req.chain)?;
    let issuance = rgb20::issue(chain, &req.name, &req.ticker, req.supply, seal)?;
    let cid = issuance.contract_id;
    Ok(IssueResult {
        contract_id: format!("{cid}"),
        chain_net: chain_label(chain).into(),
        ticker: req.ticker.clone(),
        name: req.name.clone(),
        supply: req.supply,
        seal: req.seal.clone(),
        genesis_opid_hex: hex::encode(cid.to_byte_array()),
    })
}

/// Deterministic placeholder outpoint for a future seal (spike-style).
pub fn placeholder_outpoint(label: &str) -> OutPoint {
    let h = Sha256::digest(label.as_bytes());
    let mut bytes = [0u8; 32];
    bytes.copy_from_slice(&h);
    OutPoint::new(Txid::from_byte_array(bytes), 1)
}

pub fn plan_transfer(
    contract_id_s: &str,
    supply: u64,
    send: u64,
    alice_seal_s: &str,
    bob_label: &str,
    change_label: &str,
    internal_key_hex: &str,
    entropy: u64,
    ticker: &str,
    chain: &str,
) -> Result<TransferPlan> {
    let bob_seal = placeholder_outpoint(bob_label);
    let change_seal = placeholder_outpoint(change_label);
    plan_transfer_core(
        contract_id_s,
        None,
        0,
        supply,
        send,
        alice_seal_s,
        rgb20::SealTarget::Outpoint(bob_seal),
        if supply > send {
            Some(rgb20::SealTarget::Outpoint(change_seal))
        } else {
            None
        },
        internal_key_hex,
        entropy,
        ticker,
        chain,
    )
}

/// Plan a transfer that assigns `send` units to a **known** outpoint (e.g. HTLC fund).
/// Used by S3 fund-wrap: RGB rights move onto the HTLC UTXO after value fund.
pub fn plan_transfer_to_seal(
    contract_id_s: &str,
    prev_amount: u64,
    send: u64,
    alice_seal_s: &str,
    bob_seal_s: &str,
    change_seal_s: Option<&str>,
    internal_key_hex: &str,
    entropy: u64,
    ticker: &str,
    chain: &str,
) -> Result<TransferPlan> {
    let bob = parse_outpoint(bob_seal_s)?;
    let change = match change_seal_s {
        Some(s) if prev_amount > send => Some(rgb20::SealTarget::Outpoint(parse_outpoint(s)?)),
        _ => None,
    };
    plan_transfer_core(
        contract_id_s,
        None,
        0,
        prev_amount,
        send,
        alice_seal_s,
        rgb20::SealTarget::Outpoint(bob),
        change,
        internal_key_hex,
        entropy,
        ticker,
        chain,
    )
}

/// Plan an RGB claim re-seat: close HTLC-bound seal, create successor on the
/// claim witness (`WitnessTx` vout). `prev_opid_hex` is the fund-wrap transition.
pub fn plan_claim_transfer(
    contract_id_s: &str,
    prev_opid_hex: &str,
    prev_opout_no: u16,
    prev_amount: u64,
    send: u64,
    alice_seal_s: &str,
    recipient_vout: u32,
    change_vout: Option<u32>,
    internal_key_hex: &str,
    entropy: u64,
    ticker: &str,
    chain: &str,
) -> Result<TransferPlan> {
    let prev_opid = rgbcore::OpId::from(parse32_hex(prev_opid_hex, "prev_opid")?);
    let change = change_vout.map(|v| rgb20::SealTarget::WitnessVout {
        vout: v,
        blinding: 1,
    });
    plan_transfer_core(
        contract_id_s,
        Some(prev_opid),
        prev_opout_no,
        prev_amount,
        send,
        alice_seal_s,
        rgb20::SealTarget::WitnessVout {
            vout: recipient_vout,
            blinding: 0,
        },
        change,
        internal_key_hex,
        entropy,
        ticker,
        chain,
    )
}

#[allow(clippy::too_many_arguments)]
fn plan_transfer_core(
    contract_id_s: &str,
    prev_opid: Option<rgbcore::OpId>,
    prev_opout_no: u16,
    prev_amount: u64,
    send: u64,
    alice_seal_s: &str,
    bob_target: rgb20::SealTarget,
    change_target: Option<rgb20::SealTarget>,
    internal_key_hex: &str,
    entropy: u64,
    ticker: &str,
    chain: &str,
) -> Result<TransferPlan> {
    if send == 0 || send > prev_amount {
        anyhow::bail!("send amount must be in 1..={prev_amount}");
    }
    let chain_net = parse_chain_net(chain)?;
    let contract_id = parse_contract_id(contract_id_s)?;
    let change_amount = prev_amount - send;

    let (bundle_id, transition) = if let Some(opid) = prev_opid {
        rgb20::build_transfer_from(
            contract_id,
            opid,
            prev_opout_no,
            prev_amount,
            send,
            bob_target,
            change_target,
            0,
        )?
    } else {
        rgb20::build_transfer(
            contract_id,
            prev_amount,
            send,
            bob_target,
            change_target,
            0,
        )?
    };

    let p = parse32_hex(internal_key_hex, "internal_key")?;
    let pid = contract_id.to_byte_array();
    let msg = bundle_id.to_byte_array();
    let (root, _) = mpc::build(
        &[mpc::Entry {
            protocol_id: pid,
            message: msg,
        }],
        entropy,
    )?;
    let committed = liquid_dbc::commit(p, root)?;
    let spk_bytes = hex::decode(&committed.committed_spk_hex)?;
    if !spk_bytes.starts_with(&[0x51, 0x20]) || spk_bytes.len() != 34 {
        anyhow::bail!("expected P2TR spk, got {}", committed.committed_spk_hex);
    }
    let mut q = [0u8; 32];
    q.copy_from_slice(&spk_bytes[2..34]);
    let tapret_address = tapret_addr::encode_p2tr(tapret_hrp(chain_net), &q)?;

    Ok(TransferPlan {
        contract_id: format!("{contract_id}"),
        chain_net: chain_label(chain_net).into(),
        ticker: ticker.into(),
        send_amount: send,
        change_amount,
        alice_seal: alice_seal_s.into(),
        bob_seal_placeholder: bob_target.display(),
        change_seal_placeholder: change_target
            .map(|t| t.display())
            .unwrap_or_else(|| "none".into()),
        bundle_id_hex: hex::encode(bundle_id.to_byte_array()),
        transition_opid_hex: hex::encode(transition.commit_id().to_byte_array()),
        mpc_root_hex: hex::encode(root),
        commitment_spk_hex: committed.committed_spk_hex,
        tapret_address,
        internal_key_hex: internal_key_hex.into(),
        static_entropy: entropy,
        protocol_id_hex: hex::encode(pid),
        message_hex: hex::encode(msg),
    })
}

pub fn verify_against_witness(
    plan: &TransferPlan,
    witness: &seal::WitnessTx,
    explorer_base: &str,
) -> Result<VerifyResult> {
    let mut checks = Vec::new();

    let parts: Vec<_> = plan.alice_seal.split(':').collect();
    let seal = seal::LiquidSeal {
        txid: parts.first().copied().unwrap_or("").to_string(),
        vout: parts
            .get(1)
            .and_then(|v| v.parse().ok())
            .unwrap_or(0),
    };

    match seal::verify_seal_closure(witness, &seal, &plan.commitment_spk_hex) {
        Ok(s) => checks.push(VerifyCheck {
            name: "seal_closure".into(),
            ok: true,
            detail: Some(format!(
                "seal_vin={} commitment_vout={}",
                s.seal_input_index, s.commitment_output_index
            )),
        }),
        Err(e) => checks.push(VerifyCheck {
            name: "seal_closure".into(),
            ok: false,
            detail: Some(e.to_string()),
        }),
    }

    let p = parse32_hex(&plan.internal_key_hex, "internal_key")?;
    let root = parse32_hex(&plan.mpc_root_hex, "mpc_root")?;
    match liquid_dbc::verify(&plan.commitment_spk_hex, root, p) {
        Ok(()) => checks.push(VerifyCheck {
            name: "tapret_dbc".into(),
            ok: true,
            detail: None,
        }),
        Err(e) => checks.push(VerifyCheck {
            name: "tapret_dbc".into(),
            ok: false,
            detail: Some(e.to_string()),
        }),
    }

    let pid = parse32_hex(&plan.protocol_id_hex, "protocol_id")?;
    let msg = parse32_hex(&plan.message_hex, "message")?;
    match patched_anchor::build_anchor(pid, msg, plan.static_entropy, p) {
        Ok(a) => match patched_anchor::verify_anchor_on_liquid(&a, pid, msg, witness) {
            Ok(c) => checks.push(VerifyCheck {
                name: "anchor_verify".into(),
                ok: true,
                detail: Some(format!("mpc_commitment={}", hex::encode(c))),
            }),
            Err(e) => checks.push(VerifyCheck {
                name: "anchor_verify".into(),
                ok: false,
                detail: Some(e.to_string()),
            }),
        },
        Err(e) => checks.push(VerifyCheck {
            name: "anchor_verify".into(),
            ok: false,
            detail: Some(format!("build_anchor: {e}")),
        }),
    }

    let all = checks.iter().all(|c| c.ok);
    Ok(VerifyResult {
        status: if all {
            "valid".into()
        } else {
            "invalid".into()
        },
        contract_id: Some(plan.contract_id.clone()),
        anchor_txid: witness.txid.clone(),
        checks,
        explorer_url: Some(format!(
            "{}/tx/{}",
            explorer_base.trim_end_matches('/'),
            witness.txid
        )),
    })
}

#[cfg(test)]
mod s3_plan_tests {
    use super::*;

    #[test]
    fn plan_to_seal_and_claim_chain() {
        let seal = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa:0";
        let htlc = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb:1";
        let iss = issue_nia(&IssueRequest {
            name: "S3Asset".into(),
            ticker: "s3a".into(),
            supply: 5000,
            seal: seal.into(),
            chain: "liquid-testnet".into(),
        })
        .unwrap();
        let fund = plan_transfer_to_seal(
            &iss.contract_id,
            5000,
            5000,
            seal,
            htlc,
            None,
            DEMO_INTERNAL_XONLY_HEX,
            7,
            "s3a",
            "liquid-testnet",
        )
        .unwrap();
        assert_eq!(fund.bob_seal_placeholder, htlc);
        assert_eq!(fund.send_amount, 5000);
        assert!(fund.tapret_address.starts_with("tex1") || fund.tapret_address.starts_with("tlq"));

        let claim = plan_claim_transfer(
            &iss.contract_id,
            &fund.transition_opid_hex,
            0,
            5000,
            5000,
            htlc,
            1,
            None,
            DEMO_INTERNAL_XONLY_HEX,
            8,
            "s3a",
            "liquid-testnet",
        )
        .unwrap();
        assert!(claim.bob_seal_placeholder.starts_with("witness:1:"));
        assert_ne!(claim.bundle_id_hex, fund.bundle_id_hex);
        assert_ne!(claim.commitment_spk_hex, fund.commitment_spk_hex);
    }
}
