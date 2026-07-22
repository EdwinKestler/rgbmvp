//! BFA — Backed Fungible Asset: IFA + genesis-committed backing terms + audit.
//!
//! Adapted from kaleidoswap/rgb-on-liquid-spike `bfa.rs` (MIT OR Apache-2.0).
//! See `docs/C3_CLOSED.md`.

use anyhow::{Context, Result};
use rgbcore::bitcoin::OutPoint;
use rgbcore::commit_verify::CommitId;
use rgbcore::{
    BundleId, ChainNet, ContractId, GenesisSeal, GlobalStateType, OpId, Transition,
};
use rgbstd::containers::ConsignmentExt;
use rgbstd::contract::{ContractBuilder, IssuerWrapper};
use rgbstd::rgbcore::stl::rgb_contract_id_stl;
use rgbstd::schema::{GlobalDetails, GlobalStateSchema, Occurrences, Schema};
use rgbstd::stl::{AssetSpec, ContractTerms, Details, RicardianContract, StandardTypes};
use rgbstd::validation::Scripts;
use rgbstd::{Amount, Identity, Precision};
use schemata::{InflatableFungibleAsset, GS_ISSUED_SUPPLY};
use serde::{Deserialize, Serialize};
use strict_encoding::{FieldName, TypeName};
use strict_types::TypeSystem;

use crate::mint::{self, IfaIssuance};
use crate::{liquid_dbc, mpc, GENESIS_TIMESTAMP};

/// Genesis global carrying backing terms (clear of rgb-schemas type ids).
pub const GS_BACKING: GlobalStateType = GlobalStateType::with(3100);

const TERMS_PREFIX: &str = "elements-backing:v1";

/// How backing is proven on mint witnesses (committed in genesis terms).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum BackingMode {
    /// C1: lock tranche to vault SPK.
    #[default]
    Lock,
    /// C2: destroy tranche to empty (unspendable) SPK.
    Burn,
}

impl BackingMode {
    pub fn as_str(self) -> &'static str {
        match self {
            BackingMode::Lock => "lock",
            BackingMode::Burn => "burn",
        }
    }

    pub fn parse(s: &str) -> Result<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "lock" | "vault" => Ok(BackingMode::Lock),
            "burn" => Ok(BackingMode::Burn),
            other => anyhow::bail!("unknown backing mode {other:?} (lock|burn)"),
        }
    }
}

/// Contract-committed backing terms (part of contract id).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BackingTerms {
    pub vault_spk: Vec<u8>,
    pub backing_asset: elements::AssetId,
    pub rate_num: u64,
    pub rate_den: u64,
    /// C1 lock (default) vs C2 burn — cannot silently switch after genesis.
    pub mode: BackingMode,
}

impl BackingTerms {
    pub fn to_canonical(&self) -> String {
        format!(
            "{TERMS_PREFIX};vault={};asset={};rate={}/{};mode={}",
            hex::encode(&self.vault_spk),
            self.backing_asset,
            self.rate_num,
            self.rate_den,
            self.mode.as_str()
        )
    }

    pub fn from_canonical(s: &str) -> Result<Self> {
        let mut vault = None;
        let mut asset = None;
        let mut rate = None;
        let mut mode = BackingMode::Lock;
        let mut parts = s.split(';');
        anyhow::ensure!(
            parts.next() == Some(TERMS_PREFIX),
            "backing terms must start with `{TERMS_PREFIX}`"
        );
        for part in parts {
            match part.split_once('=') {
                Some(("vault", v)) => vault = Some(hex::decode(v).context("vault spk hex")?),
                Some(("asset", v)) => {
                    asset = Some(v.parse::<elements::AssetId>().context("backing asset id")?)
                }
                Some(("rate", v)) => {
                    let (n, d) = v.split_once('/').context("rate must be <num>/<den>")?;
                    rate = Some((n.parse::<u64>()?, d.parse::<u64>()?));
                }
                Some(("mode", v)) => mode = BackingMode::parse(v)?,
                _ => anyhow::bail!("unknown backing terms field: {part}"),
            }
        }
        let (rate_num, rate_den) = rate.context("missing rate")?;
        anyhow::ensure!(rate_den > 0, "rate denominator must be non-zero");
        let vault_spk = vault.context("missing vault")?;
        if mode == BackingMode::Burn {
            anyhow::ensure!(
                vault_spk.is_empty(),
                "mode=burn requires empty vault SPK (unspendable burn target)"
            );
        }
        Ok(Self {
            vault_spk,
            backing_asset: asset.context("missing asset")?,
            rate_num,
            rate_den,
            mode,
        })
    }

    /// `ceil(minted * rate_num / rate_den)`.
    pub fn required_backing(&self, minted: u64) -> Result<u64> {
        let num = (minted as u128) * (self.rate_num as u128);
        let den = self.rate_den as u128;
        u64::try_from(num.div_ceil(den)).context("required backing overflows u64")
    }
}

pub fn bfa_schema() -> Schema {
    let types = StandardTypes::with(rgb_contract_id_stl());
    let mut schema = InflatableFungibleAsset::schema();
    schema.name = TypeName::try_from("BackedFungibleAsset".to_owned()).expect("valid type name");
    schema
        .global_types
        .insert(
            GS_BACKING,
            GlobalDetails {
                global_state_schema: GlobalStateSchema::once(types.get("RGBContract.Details")),
                name: FieldName::from("backingTerms"),
            },
        )
        .expect("schema global types within confinement");
    schema
        .genesis
        .globals
        .insert(GS_BACKING, Occurrences::Once)
        .expect("genesis globals within confinement");
    schema
}

pub fn bfa_types() -> TypeSystem {
    StandardTypes::with(rgb_contract_id_stl()).type_system(bfa_schema())
}

pub fn bfa_scripts() -> Scripts {
    InflatableFungibleAsset::scripts()
}

pub fn issue_bfa(
    chain_net: ChainNet,
    name: &str,
    ticker: &str,
    max_supply: u64,
    gate_seal: OutPoint,
    terms: &BackingTerms,
) -> Result<IfaIssuance> {
    use rgbstd::stl::{Name, Ticker};

    let spec = AssetSpec {
        ticker: Ticker::try_from(ticker.to_owned()).map_err(|e| anyhow::anyhow!("ticker: {e}"))?,
        name: Name::try_from(name.to_owned()).map_err(|e| anyhow::anyhow!("name: {e}"))?,
        details: None,
        precision: Precision::Indivisible,
    };
    let contract_terms = ContractTerms {
        text: RicardianContract::default(),
        media: None,
    };
    let backing = Details::try_from(terms.to_canonical())
        .map_err(|e| anyhow::anyhow!("backing terms as Details: {e}"))?;

    let gate: GenesisSeal = GenesisSeal::with_blinding(gate_seal.txid, gate_seal.vout, 0u64);

    let consignment = ContractBuilder::with(
        Identity::default(),
        bfa_schema(),
        bfa_types(),
        bfa_scripts(),
        chain_net,
    )
    .add_global_state(FieldName::from("spec"), spec)
    .context("add spec")?
    .add_global_state(FieldName::from("terms"), contract_terms)
    .context("add terms")?
    .add_global_state(FieldName::from("backingTerms"), backing)
    .context("add backingTerms")?
    .add_global_state(FieldName::from("issuedSupply"), Amount::from(0u64))
    .context("add issuedSupply")?
    .add_global_state(FieldName::from("maxSupply"), Amount::from(max_supply))
    .context("add maxSupply")?
    .add_fungible_state(FieldName::from("inflationAllowance"), gate, max_supply)
    .context("add inflationAllowance")?
    .issue_contract_raw(GENESIS_TIMESTAMP)
    .map_err(|e| anyhow::anyhow!("issue_contract: {e:?}"))?;

    Ok(IfaIssuance {
        contract_id: consignment.contract_id(),
        gate_seal_outpoint: gate_seal,
        max_supply,
    })
}

#[allow(clippy::too_many_arguments)]
pub fn build_bfa_mint(
    contract_id: ContractId,
    gate_opid: OpId,
    gate_opout_no: u16,
    allowance_before: u64,
    mint_amount: u64,
    recipient_seal: OutPoint,
    new_gate_seal: Option<OutPoint>,
) -> Result<(BundleId, Transition)> {
    mint::build_mint_with(
        bfa_schema(),
        bfa_types(),
        contract_id,
        gate_opid,
        gate_opout_no,
        allowance_before,
        mint_amount,
        recipient_seal,
        new_gate_seal,
    )
}

pub fn minted_amount(transition: &Transition) -> Result<u64> {
    let values = transition
        .globals
        .get(&GS_ISSUED_SUPPLY)
        .context("TS_INFLATION transition has no issuedSupply global")?;
    let value = values.first().context("empty issuedSupply global")?;
    let bytes: &[u8] = value.as_ref();
    anyhow::ensure!(
        bytes.len() == 8,
        "issuedSupply must be a strict-encoded u64, got {} bytes",
        bytes.len()
    );
    let mut le = [0u8; 8];
    le.copy_from_slice(bytes);
    Ok(u64::from_le_bytes(le))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MintAudit {
    pub minted: u64,
    pub required: u64,
    pub locked: u64,
}

pub fn audit_mint(
    terms: &BackingTerms,
    transition: &Transition,
    witness_tx: &elements::Transaction,
) -> Result<MintAudit> {
    let minted = minted_amount(transition)?;
    let required = terms.required_backing(minted)?;
    let locked =
        mint::verify_backing(witness_tx, &terms.vault_spk, &terms.backing_asset, required)?;
    Ok(MintAudit {
        minted,
        required,
        locked,
    })
}

// ── Full-history audit (rebuild + seal + anchor + backing) ───────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BfaHistoryMint {
    pub mint: u64,
    pub recipient_seal: String,
    pub new_gate_seal: Option<String>,
    pub witness_txid: String,
    /// Optional raw Elements tx hex (offline audit without RPC).
    #[serde(default)]
    pub witness_tx_hex: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BfaHistory {
    pub name: String,
    pub ticker: String,
    pub max_supply: u64,
    pub backing: String,
    pub genesis_gate_seal: String,
    pub internal_key: String,
    pub entropy: u64,
    #[serde(default = "default_chain")]
    pub chain_net: String,
    pub mints: Vec<BfaHistoryMint>,
}

fn default_chain() -> String {
    "liquid-testnet".into()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MintCheckResult {
    pub mint_index: usize,
    pub seal_ok: bool,
    pub anchor_ok: bool,
    pub backing_ok: bool,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BfaAuditResult {
    pub contract_id: String,
    pub ok: bool,
    pub failures: usize,
    pub mints: Vec<MintCheckResult>,
    pub summary: String,
}

fn parse_outpoint(s: &str) -> Result<OutPoint> {
    use rgbcore::bitcoin::hashes::Hash;
    use rgbcore::bitcoin::Txid;
    let (txid_s, vout_s) = s
        .split_once(':')
        .context("outpoint must be txid:vout")?;
    let bytes = hex::decode(txid_s).context("txid hex")?;
    anyhow::ensure!(bytes.len() == 32, "txid must be 32 bytes");
    let mut arr = [0u8; 32];
    arr.copy_from_slice(&bytes);
    let txid = Txid::from_byte_array(arr);
    let vout: u32 = vout_s.parse().context("vout")?;
    Ok(OutPoint::new(txid, vout))
}

fn parse32(s: &str) -> Result<[u8; 32]> {
    let s = s.trim().trim_start_matches("0x");
    let b = hex::decode(s).context("hex32")?;
    anyhow::ensure!(b.len() == 32, "need 32 bytes");
    let mut a = [0u8; 32];
    a.copy_from_slice(&b);
    Ok(a)
}

/// Resolve witness tx bytes from history entry (embedded hex or caller-supplied map).
pub fn resolve_witness_hex(
    m: &BfaHistoryMint,
    fetch: &dyn Fn(&str) -> Result<String>,
) -> Result<String> {
    if let Some(h) = &m.witness_tx_hex {
        if !h.is_empty() {
            return Ok(h.clone());
        }
    }
    fetch(&m.witness_txid)
}

/// Full-history BFA audit. `fetch_tx` is called when `witness_tx_hex` is absent.
pub fn audit_history(
    history: &BfaHistory,
    fetch_tx: &dyn Fn(&str) -> Result<String>,
) -> Result<BfaAuditResult> {
    let terms = BackingTerms::from_canonical(&history.backing)?;
    let chain_net = crate::parse_chain_net(&history.chain_net)?;
    let p = parse32(&history.internal_key)?;
    let genesis_gate = parse_outpoint(&history.genesis_gate_seal)?;

    let issuance = issue_bfa(
        chain_net,
        &history.name,
        &history.ticker,
        history.max_supply,
        genesis_gate,
        &terms,
    )?;

    let mut consume_opid = OpId::from(issuance.contract_id.to_byte_array());
    let mut allowance = history.max_supply;
    // Track gate as Elements-display `txid:vout` string — do not round-trip
    // through bitcoin::Txid Display (byte-order differs from Elements RPC).
    let mut gate_s = history.genesis_gate_seal.clone();
    let mut failures = 0usize;
    let mut mint_results = Vec::new();

    for (i, m) in history.mints.iter().enumerate() {
        let n = i + 1;
        let recipient = parse_outpoint(&m.recipient_seal)?;
        let new_gate = m
            .new_gate_seal
            .as_deref()
            .map(parse_outpoint)
            .transpose()?;

        let (bundle_id, transition) = build_bfa_mint(
            issuance.contract_id,
            consume_opid,
            0,
            allowance,
            m.mint,
            recipient,
            new_gate,
        )?;

        let entries = vec![mpc::Entry {
            protocol_id: issuance.contract_id.to_byte_array(),
            message: bundle_id.to_byte_array(),
        }];
        let (root, _) = mpc::build(&entries, history.entropy)?;
        let committed = liquid_dbc::commit(p, root)?;

        let raw_hex = resolve_witness_hex(m, fetch_tx)
            .with_context(|| format!("mint #{n}: witness {}", m.witness_txid))?;
        let witness_tx: elements::Transaction =
            elements::encode::deserialize(&hex::decode(raw_hex.trim()).context("tx hex")?)
                .map_err(|e| anyhow::anyhow!("deserialize witness tx: {e}"))?;

        let closes_gate = witness_tx.input.iter().any(|txin| {
            let spent = format!("{}:{}", txin.previous_output.txid, txin.previous_output.vout);
            spent == gate_s
        });

        let anchored = witness_tx.output.iter().any(|o| {
            hex::encode(o.script_pubkey.as_bytes()) == committed.committed_spk_hex
        });

        let mut detail = String::new();
        let seal_ok = closes_gate;
        let anchor_ok = anchored;
        let mut backing_ok = false;

        if seal_ok {
            detail.push_str("seal ok; ");
        } else {
            detail.push_str(&format!("SEAL fail gate {gate_s} ; "));
            failures += 1;
        }
        if anchor_ok {
            detail.push_str("anchor ok; ");
        } else {
            detail.push_str("ANCHOR fail; ");
            failures += 1;
        }
        if anchored {
            match audit_mint(&terms, &transition, &witness_tx) {
                Ok(a) => {
                    backing_ok = true;
                    let label = match terms.mode {
                        BackingMode::Burn => "burned",
                        BackingMode::Lock => "locked",
                    };
                    detail.push_str(&format!(
                        "backing ok (minted {} req {} {label} {}; mode={})",
                        a.minted,
                        a.required,
                        a.locked,
                        terms.mode.as_str()
                    ));
                }
                Err(e) => {
                    detail.push_str(&format!("BACKING fail: {e}"));
                    failures += 1;
                }
            }
        } else {
            detail.push_str("backing skipped (no anchor)");
        }

        mint_results.push(MintCheckResult {
            mint_index: n,
            seal_ok,
            anchor_ok,
            backing_ok,
            detail,
        });

        consume_opid = transition.commit_id();
        allowance = allowance
            .checked_sub(m.mint)
            .context("history mints exceed max supply")?;
        gate_s = match &m.new_gate_seal {
            Some(g) => g.clone(),
            None => {
                anyhow::ensure!(
                    i == history.mints.len() - 1 || allowance == 0,
                    "mint #{n} has no new gate seal but allowance remains"
                );
                gate_s
            }
        };
    }

    let ok = failures == 0;
    Ok(BfaAuditResult {
        contract_id: format!("{}", issuance.contract_id),
        ok,
        failures,
        mints: mint_results,
        summary: if ok {
            format!("audit OK: {} mints, fully backed", history.mints.len())
        } else {
            format!("audit FAILED: {failures} check(s) failed")
        },
    })
}

/// Parse `txid:vout` into an OutPoint.
pub fn parse_outpoint_str(s: &str) -> Result<OutPoint> {
    parse_outpoint(s)
}

/// Plan a BFA mint and return operator-facing JSON fields.
#[allow(clippy::too_many_arguments)]
pub fn plan_mint_json(
    name: &str,
    ticker: &str,
    max_supply: u64,
    backing_canonical: &str,
    genesis_gate: &str,
    gate_seal: &str,
    mint: u64,
    recipient_seal: &str,
    new_gate_seal: &str,
    consume_opid_hex: Option<&str>,
    allowance: Option<u64>,
    internal_key_hex: &str,
    entropy: u64,
    chain: &str,
) -> Result<serde_json::Value> {
    let terms = BackingTerms::from_canonical(backing_canonical)?;
    let chain_net = crate::parse_chain_net(chain)?;
    let g0 = parse_outpoint(genesis_gate)?;
    let iss = issue_bfa(chain_net, name, ticker, max_supply, g0, &terms)?;
    let gate_opid = if let Some(h) = consume_opid_hex {
        let b = parse32(h)?;
        OpId::from(b)
    } else {
        OpId::from(iss.contract_id.to_byte_array())
    };
    let allow = allowance.unwrap_or(max_supply);
    let (bundle_id, transition) = build_bfa_mint(
        iss.contract_id,
        gate_opid,
        0,
        allow,
        mint,
        parse_outpoint(recipient_seal)?,
        Some(parse_outpoint(new_gate_seal)?),
    )?;
    let (root, spk, addr) =
        mint_commitment(iss.contract_id, bundle_id, internal_key_hex, entropy, chain)?;
    Ok(serde_json::json!({
        "contract_id": format!("{}", iss.contract_id),
        "opid_hex": hex::encode(transition.commit_id().to_byte_array()),
        "bundle_id_hex": hex::encode(bundle_id.to_byte_array()),
        "mpc_root_hex": root,
        "commitment_spk_hex": spk,
        "tapret_address": addr,
        "mint": mint,
        "gate_seal": gate_seal,
        "recipient_seal": recipient_seal,
        "new_gate_seal": new_gate_seal,
        "allowance_after": allow.saturating_sub(mint),
    }))
}

/// Issue helper returning JSON.
pub fn issue_json(
    name: &str,
    ticker: &str,
    max_supply: u64,
    gate_seal: &str,
    backing_canonical: &str,
    chain: &str,
) -> Result<serde_json::Value> {
    let terms = BackingTerms::from_canonical(backing_canonical)?;
    let chain_net = crate::parse_chain_net(chain)?;
    let gate = parse_outpoint(gate_seal)?;
    let iss = issue_bfa(chain_net, name, ticker, max_supply, gate, &terms)?;
    Ok(serde_json::json!({
        "contract_id": format!("{}", iss.contract_id),
        "gate_seal": gate_seal,
        "max_supply": max_supply,
        "backing": terms.to_canonical(),
        "schema": "BackedFungibleAsset",
    }))
}

/// Build tapret commitment address fields for a planned mint (for demos).
pub fn mint_commitment(
    contract_id: ContractId,
    bundle_id: BundleId,
    internal_key_hex: &str,
    entropy: u64,
    chain: &str,
) -> Result<(String, String, String)> {
    let p = parse32(internal_key_hex)?;
    let entries = vec![mpc::Entry {
        protocol_id: contract_id.to_byte_array(),
        message: bundle_id.to_byte_array(),
    }];
    let (root, _) = mpc::build(&entries, entropy)?;
    let committed = liquid_dbc::commit(p, root)?;
    let spk_bytes = hex::decode(&committed.committed_spk_hex)?;
    anyhow::ensure!(spk_bytes.len() == 34 && spk_bytes[0] == 0x51, "P2TR spk");
    let mut q = [0u8; 32];
    q.copy_from_slice(&spk_bytes[2..34]);
    let hrp = crate::tapret_hrp(crate::parse_chain_net(chain)?);
    // Elements regtest demos use ert; allow override via liquid-testnet → tex / use ELEMENTS
    let addr = if chain.contains("regtest") || chain == "elements-regtest" {
        crate::tapret_addr::encode_p2tr("ert", &q)?
    } else {
        crate::tapret_addr::encode_p2tr(hrp, &q)?
    };
    Ok((
        hex::encode(root),
        committed.committed_spk_hex,
        addr,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rgbcore::bitcoin::{hashes::Hash, Txid};

    fn outpoint(seed: u8) -> OutPoint {
        let mut bytes = [0u8; 32];
        bytes[0] = seed;
        OutPoint::new(Txid::from_byte_array(bytes), 0)
    }

    fn demo_terms() -> BackingTerms {
        BackingTerms {
            vault_spk: hex::decode("0014aabbccddeeff00112233445566778899aabbccdd").unwrap(),
            backing_asset: "5ac9f65c0efcc4775e0baec4ec03abdde22473cd3cf33c0419ca290e0751b225"
                .parse()
                .unwrap(),
            rate_num: 1,
            rate_den: 1,
            mode: BackingMode::Lock,
        }
    }

    #[test]
    fn terms_roundtrip() {
        let t = demo_terms();
        let s = t.to_canonical();
        assert!(s.len() <= 255);
        assert!(s.contains("mode=lock"));
        assert_eq!(BackingTerms::from_canonical(&s).unwrap(), t);
    }

    #[test]
    fn burn_terms_require_empty_vault() {
        let burn = BackingTerms {
            vault_spk: vec![],
            backing_asset: demo_terms().backing_asset,
            rate_num: 1,
            rate_den: 1,
            mode: BackingMode::Burn,
        };
        let s = burn.to_canonical();
        assert!(s.contains("mode=burn"));
        assert_eq!(BackingTerms::from_canonical(&s).unwrap(), burn);
        let bad = format!(
            "{TERMS_PREFIX};vault=0014aa;asset={};rate=1/1;mode=burn",
            burn.backing_asset
        );
        assert!(BackingTerms::from_canonical(&bad).is_err());
    }

    #[test]
    fn required_backing_rounds_up() {
        let mut t = demo_terms();
        t.rate_num = 1;
        t.rate_den = 3;
        assert_eq!(t.required_backing(10).unwrap(), 4);
        t.rate_num = 2;
        t.rate_den = 1;
        assert_eq!(t.required_backing(10).unwrap(), 20);
    }

    #[test]
    fn bfa_is_distinct_schema_and_terms_move_contract_id() {
        let schema = bfa_schema();
        assert_ne!(schema.schema_id(), schemata::IFA_SCHEMA_ID);

        let a = issue_bfa(
            ChainNet::LiquidTestnet,
            "LiquidRgbUSD",
            "LRUSD",
            1_000_000,
            outpoint(0x31),
            &demo_terms(),
        )
        .expect("issue");
        let mut other = demo_terms();
        other.rate_num = 2;
        let b = issue_bfa(
            ChainNet::LiquidTestnet,
            "LiquidRgbUSD",
            "LRUSD",
            1_000_000,
            outpoint(0x31),
            &other,
        )
        .expect("issue");
        assert_ne!(a.contract_id, b.contract_id);
    }

    #[test]
    fn mint_commits_its_size() {
        let issuance = issue_bfa(
            ChainNet::LiquidTestnet,
            "LiquidRgbUSD",
            "LRUSD",
            1_000_000,
            outpoint(0x41),
            &demo_terms(),
        )
        .unwrap();
        let genesis_opid = OpId::from(issuance.contract_id.to_byte_array());
        let (_b, transition) = build_bfa_mint(
            issuance.contract_id,
            genesis_opid,
            0,
            1_000_000,
            250_000,
            outpoint(0x42),
            Some(outpoint(0x43)),
        )
        .unwrap();
        assert_eq!(minted_amount(&transition).unwrap(), 250_000);
    }

    #[test]
    fn audit_math_binds_mint_to_backing() {
        use elements::confidential::{Asset, Nonce, Value};
        let terms = demo_terms();
        let issuance = issue_bfa(
            ChainNet::LiquidTestnet,
            "LiquidRgbUSD",
            "LRUSD",
            1_000_000,
            outpoint(0x51),
            &terms,
        )
        .unwrap();
        let genesis_opid = OpId::from(issuance.contract_id.to_byte_array());
        let (_b, transition) = build_bfa_mint(
            issuance.contract_id,
            genesis_opid,
            0,
            1_000_000,
            30_000,
            outpoint(0x52),
            Some(outpoint(0x53)),
        )
        .unwrap();

        let vault_out = |amount: u64| elements::TxOut {
            asset: Asset::Explicit(terms.backing_asset),
            value: Value::Explicit(amount),
            nonce: Nonce::Null,
            script_pubkey: elements::Script::from(terms.vault_spk.clone()),
            witness: elements::TxOutWitness::default(),
        };
        let tx = |vault_amount: u64| elements::Transaction {
            version: 2,
            lock_time: elements::LockTime::ZERO,
            input: vec![],
            output: vec![vault_out(vault_amount)],
        };

        let ok = audit_mint(&terms, &transition, &tx(30_000)).unwrap();
        assert_eq!((ok.minted, ok.required, ok.locked), (30_000, 30_000, 30_000));
        let err = audit_mint(&terms, &transition, &tx(29_999)).unwrap_err();
        assert!(err.to_string().contains("under-backed"), "got: {err}");
    }
}
