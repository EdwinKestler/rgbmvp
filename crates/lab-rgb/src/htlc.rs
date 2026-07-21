//! Cross-chain HTLC scripts + spend builders (Bitcoin).
//!
//! Script layout matches kaleidoswap/rgb-on-liquid-spike (claim + CSV refund).

use anyhow::{bail, Context, Result};
use bitcoin::absolute::LockTime;
use bitcoin::consensus::encode::serialize_hex;
use bitcoin::hashes::{sha256, Hash};
use bitcoin::key::Secp256k1;
use bitcoin::secp256k1::{Message, PublicKey, SecretKey};
use bitcoin::sighash::{EcdsaSighashType, SighashCache};
use bitcoin::transaction::Version;
use bitcoin::{Amount, OutPoint, ScriptBuf, Sequence, Transaction, TxIn, TxOut, Txid, Witness};
use serde::{Deserialize, Serialize};

pub enum HtlcSpend<'a> {
    Claim { preimage: &'a [u8] },
    Refund,
}

/// OP_IF OP_SHA256 <H> OP_EQUALVERIFY <claimerPk> OP_CHECKSIG
/// OP_ELSE <T> OP_CSV OP_DROP <refundPk> OP_CHECKSIG OP_ENDIF
pub fn htlc_witness_script(
    hash: &[u8; 32],
    claimer_pk: &[u8; 33],
    refund_pk: &[u8; 33],
    csv_delay: u32,
) -> Vec<u8> {
    let mut s = Vec::with_capacity(112);
    s.push(0x63); // OP_IF
    s.push(0xa8); // OP_SHA256
    s.push(0x20);
    s.extend_from_slice(hash);
    s.push(0x88); // OP_EQUALVERIFY
    s.push(0x21);
    s.extend_from_slice(claimer_pk);
    s.push(0xac); // OP_CHECKSIG
    s.push(0x67); // OP_ELSE
    s.extend_from_slice(&push_script_num(csv_delay));
    s.push(0xb2); // OP_CHECKSEQUENCEVERIFY
    s.push(0x75); // OP_DROP
    s.push(0x21);
    s.extend_from_slice(refund_pk);
    s.push(0xac); // OP_CHECKSIG
    s.push(0x68); // OP_ENDIF
    s
}

fn push_script_num(n: u32) -> Vec<u8> {
    assert!(n > 0, "CSV delay must be positive");
    if n <= 16 {
        return vec![0x50 + n as u8];
    }
    let mut le = Vec::new();
    let mut v = n;
    while v > 0 {
        le.push((v & 0xff) as u8);
        v >>= 8;
    }
    if le.last().copied().unwrap_or(0) & 0x80 != 0 {
        le.push(0x00);
    }
    let mut out = vec![le.len() as u8];
    out.extend_from_slice(&le);
    out
}

pub fn p2wsh_spk(witness_script: &[u8]) -> Vec<u8> {
    let wsh = sha256::Hash::hash(witness_script);
    let mut spk = Vec::with_capacity(34);
    spk.push(0x00);
    spk.push(0x20);
    spk.extend_from_slice(wsh.as_byte_array());
    spk
}

pub fn p2wsh_address(network_hrp: &str, witness_script: &[u8]) -> Result<String> {
    use bech32::{segwit, Hrp};
    let wsh = sha256::Hash::hash(witness_script);
    let hrp = Hrp::parse(network_hrp).map_err(|e| anyhow::anyhow!("hrp: {e}"))?;
    let v0 = bech32::Fe32::try_from(0u8).unwrap();
    segwit::encode(hrp, v0, wsh.as_byte_array()).map_err(|e| anyhow::anyhow!("bech32: {e}"))
}

/// Deterministic demo key from label (testnet only).
pub fn demo_keypair(label: &str) -> Result<(SecretKey, [u8; 33])> {
    let secp = Secp256k1::new();
    let sk_bytes = sha256::Hash::hash(label.as_bytes());
    let sk = SecretKey::from_slice(sk_bytes.as_byte_array())
        .context("invalid secret from label hash")?;
    let pk = PublicKey::from_secret_key(&secp, &sk);
    Ok((sk, pk.serialize()))
}

pub fn sha256_preimage(preimage: &[u8]) -> [u8; 32] {
    sha256::Hash::hash(preimage).to_byte_array()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HtlcAddressInfo {
    pub hash_hex: String,
    pub csv_delay: u32,
    pub claimer_label: String,
    pub refund_label: String,
    pub witness_script_hex: String,
    pub spk_hex: String,
    pub address_btc: String,
    pub address_liquid_unconf: String,
}

pub fn build_htlc_addresses(
    hash: &[u8; 32],
    claimer_label: &str,
    refund_label: &str,
    csv_delay: u32,
) -> Result<HtlcAddressInfo> {
    let (_, claimer_pk) = demo_keypair(claimer_label)?;
    let (_, refund_pk) = demo_keypair(refund_label)?;
    let ws = htlc_witness_script(hash, &claimer_pk, &refund_pk, csv_delay);
    Ok(HtlcAddressInfo {
        hash_hex: hex::encode(hash),
        csv_delay,
        claimer_label: claimer_label.into(),
        refund_label: refund_label.into(),
        witness_script_hex: hex::encode(&ws),
        spk_hex: hex::encode(p2wsh_spk(&ws)),
        address_btc: p2wsh_address("tb", &ws)?,
        address_liquid_unconf: p2wsh_address("tex", &ws)?,
    })
}

fn htlc_sequence(spend: &HtlcSpend, csv_delay: u32) -> u32 {
    match spend {
        HtlcSpend::Claim { .. } => 0xffff_fffd,
        HtlcSpend::Refund => csv_delay,
    }
}

fn htlc_witness_stack(sig_der_all: Vec<u8>, spend: &HtlcSpend, ws: &[u8]) -> Vec<Vec<u8>> {
    match spend {
        HtlcSpend::Claim { preimage } => {
            vec![sig_der_all, preimage.to_vec(), vec![0x01], ws.to_vec()]
        }
        HtlcSpend::Refund => vec![sig_der_all, vec![], ws.to_vec()],
    }
}

/// Build + sign Bitcoin HTLC spend (claim or refund). Returns raw tx hex.
#[allow(clippy::too_many_arguments)]
pub fn build_htlc_spend_btc(
    prev_txid: &str,
    prev_vout: u32,
    input_value_sat: u64,
    output_value_sat: u64,
    dest_spk: &[u8],
    witness_script: &[u8],
    spend: HtlcSpend,
    csv_delay: u32,
    signer_sk: &SecretKey,
) -> Result<String> {
    if output_value_sat >= input_value_sat {
        bail!("output must leave room for fee");
    }
    let prev_txid: Txid = prev_txid.parse().context("prev_txid")?;
    let mut tx = Transaction {
        version: Version::TWO,
        lock_time: LockTime::ZERO,
        input: vec![TxIn {
            previous_output: OutPoint::new(prev_txid, prev_vout),
            script_sig: ScriptBuf::new(),
            sequence: Sequence::from_consensus(htlc_sequence(&spend, csv_delay)),
            witness: Witness::new(),
        }],
        output: vec![TxOut {
            value: Amount::from_sat(output_value_sat),
            script_pubkey: ScriptBuf::from_bytes(dest_spk.to_vec()),
        }],
    };

    let sighash = SighashCache::new(&tx)
        .p2wsh_signature_hash(
            0,
            ScriptBuf::from_bytes(witness_script.to_vec()).as_script(),
            Amount::from_sat(input_value_sat),
            EcdsaSighashType::All,
        )
        .context("p2wsh sighash")?;

    let secp = Secp256k1::new();
    let msg = Message::from_digest(sighash.to_byte_array());
    let mut sig = secp.sign_ecdsa(&msg, signer_sk).serialize_der().to_vec();
    sig.push(EcdsaSighashType::All as u8);

    for item in htlc_witness_stack(sig, &spend, witness_script) {
        tx.input[0].witness.push(item);
    }
    Ok(serialize_hex(&tx))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn htlc_deterministic() {
        let h = [0x42u8; 32];
        let a = build_htlc_addresses(&h, "claimer", "refund", 6).unwrap();
        let b = build_htlc_addresses(&h, "claimer", "refund", 6).unwrap();
        assert_eq!(a.address_btc, b.address_btc);
        assert!(a.address_btc.starts_with("tb1q"));
    }

    #[test]
    fn preimage_hash() {
        let s = [0x11u8; 32];
        assert_ne!(sha256_preimage(&s), [0u8; 32]);
    }
}

/// Build + sign Elements/Liquid HTLC claim/refund (explicit L-BTC only).
#[allow(clippy::too_many_arguments)]
pub fn build_htlc_spend_liquid(
    prev_txid: &str,
    prev_vout: u32,
    input_value_sat: u64,
    output_value_sat: u64,
    fee_sat: u64,
    dest_spk: &[u8],
    lbtc_asset_hex: &str,
    witness_script: &[u8],
    spend: HtlcSpend,
    csv_delay: u32,
    signer_sk: &SecretKey,
) -> Result<String> {
    use elements::confidential::{Asset, Nonce, Value};
    use elements::encode::serialize_hex;
    use elements::hashes::Hash as _;
    use elements::sighash::SighashCache;
    use elements::{
        AssetId, EcdsaSighashType, OutPoint, Script, Sequence, Transaction, TxIn, TxInWitness,
        TxOut, TxOutWitness, Txid,
    };
    use elements::secp256k1_zkp::Message as ElMessage;
    use elements::secp256k1_zkp::Secp256k1 as ElSecp;

    if output_value_sat + fee_sat != input_value_sat {
        bail!(
            "LQ claim: output+fee must equal input ({output_value_sat}+{fee_sat} != {input_value_sat})"
        );
    }
    let txid: Txid = prev_txid.parse().context("prev_txid")?;
    let asset_id: AssetId = lbtc_asset_hex.parse().context("asset id")?;
    let lbtc = Asset::Explicit(asset_id);

    let input = TxIn {
        previous_output: OutPoint::new(txid, prev_vout),
        is_pegin: false,
        script_sig: Script::new(),
        sequence: Sequence::from_consensus(htlc_sequence(&spend, csv_delay)),
        asset_issuance: Default::default(),
        witness: TxInWitness::default(),
    };

    let mut output = vec![
        TxOut {
            asset: lbtc,
            value: Value::Explicit(output_value_sat),
            nonce: Nonce::Null,
            script_pubkey: Script::from(dest_spk.to_vec()),
            witness: TxOutWitness::default(),
        },
        TxOut {
            asset: lbtc,
            value: Value::Explicit(fee_sat),
            nonce: Nonce::Null,
            script_pubkey: Script::new(),
            witness: TxOutWitness::default(),
        },
    ];

    let mut tx = Transaction {
        version: 2,
        lock_time: elements::LockTime::ZERO,
        input: vec![input],
        output,
    };

    let sighash = SighashCache::new(&tx).segwitv0_sighash(
        0,
        &Script::from(witness_script.to_vec()),
        Value::Explicit(input_value_sat),
        EcdsaSighashType::All,
    );

    // Sign with bitcoin SecretKey bytes via elements secp
    let el_secp = ElSecp::new();
    let el_sk = elements::secp256k1_zkp::SecretKey::from_slice(&signer_sk.secret_bytes())
        .context("el secret key")?;
    let msg = ElMessage::from_digest(sighash.to_byte_array());
    let mut sig = el_secp.sign_ecdsa(&msg, &el_sk).serialize_der().to_vec();
    sig.push(EcdsaSighashType::All as u8);
    tx.input[0].witness.script_witness = htlc_witness_stack(sig, &spend, witness_script);
    Ok(serialize_hex(&tx))
}
