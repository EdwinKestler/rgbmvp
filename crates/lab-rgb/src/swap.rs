//! Swap session state for BTC ↔ Liquid RGB atomic swaps (P1).

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

use crate::htlc::{self, HtlcAddressInfo};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SwapPhase {
    Created,
    FundedBtc,
    FundedLq,
    /// Both legs funded (either order ends here via transition helpers)
    FundedBoth,
    ClaimedLq,
    ClaimedBtc,
    Done,
    Refunded,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwapSession {
    pub id: String,
    pub phase: SwapPhase,
    pub csv_delay: u32,
    pub preimage_hex: String,
    pub hash_hex: String,
    pub btc_contract_id: Option<String>,
    pub lq_contract_id: Option<String>,
    pub alice_btc_wallet: String,
    pub bob_lq_wallet: String,
    /// BTC leg: Bob claims, Alice refunds
    pub htlc_btc: HtlcAddressInfo,
    /// Liquid leg: Alice claims, Bob refunds
    pub htlc_lq: HtlcAddressInfo,
    pub btc_fund_txid: Option<String>,
    pub btc_fund_vout: Option<u32>,
    pub btc_fund_sats: Option<u64>,
    pub lq_fund_txid: Option<String>,
    pub lq_fund_vout: Option<u32>,
    pub lq_fund_sats: Option<u64>,
    pub lq_claim_txid: Option<String>,
    pub btc_claim_txid: Option<String>,
    pub notes: Vec<String>,
}

pub struct SwapStore {
    root: PathBuf,
}

impl SwapStore {
    pub fn new(data_dir: impl AsRef<Path>) -> Self {
        Self {
            root: data_dir.as_ref().join("swaps"),
        }
    }

    pub fn ensure(&self) -> Result<()> {
        fs::create_dir_all(&self.root)?;
        Ok(())
    }

    fn path(&self, id: &str) -> PathBuf {
        self.root.join(format!("{id}.json"))
    }

    pub fn save(&self, s: &SwapSession) -> Result<PathBuf> {
        self.ensure()?;
        let p = self.path(&s.id);
        fs::write(&p, serde_json::to_vec_pretty(s)?)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&p)?.permissions();
            perms.set_mode(0o600);
            fs::set_permissions(&p, perms)?;
        }
        Ok(p)
    }

    pub fn load(&self, id: &str) -> Result<SwapSession> {
        let p = self.path(id);
        let raw = fs::read_to_string(&p).with_context(|| format!("load {}", p.display()))?;
        Ok(serde_json::from_str(&raw)?)
    }

    pub fn path_exists(&self, id: &str) -> bool {
        self.path(id).exists()
    }
}

pub fn init_swap(
    id: &str,
    csv_delay: u32,
    alice_btc_wallet: &str,
    bob_lq_wallet: &str,
    btc_contract_id: Option<String>,
    lq_contract_id: Option<String>,
) -> Result<SwapSession> {
    if csv_delay == 0 {
        bail!("csv_delay must be > 0");
    }
    let mut preimage = [0u8; 32];
    fill_random(&mut preimage)?;
    let hash = htlc::sha256_preimage(&preimage);

    // BTC: Bob claims Alice's locked coins; Alice refunds after CSV
    let htlc_btc = htlc::build_htlc_addresses(&hash, "bob-claimer", "alice-refund", csv_delay)?;
    // LQ: Alice claims Bob's locked L-BTC; Bob refunds after CSV
    let htlc_lq = htlc::build_htlc_addresses(&hash, "alice-claimer", "bob-refund", csv_delay)?;

    Ok(SwapSession {
        id: id.into(),
        phase: SwapPhase::Created,
        csv_delay,
        preimage_hex: hex::encode(preimage),
        hash_hex: hex::encode(hash),
        btc_contract_id,
        lq_contract_id,
        alice_btc_wallet: alice_btc_wallet.into(),
        bob_lq_wallet: bob_lq_wallet.into(),
        htlc_btc,
        htlc_lq,
        btc_fund_txid: None,
        btc_fund_vout: None,
        btc_fund_sats: None,
        lq_fund_txid: None,
        lq_fund_vout: None,
        lq_fund_sats: None,
        lq_claim_txid: None,
        btc_claim_txid: None,
        notes: vec![
            "Alice claims Liquid first (reveals preimage). Bob then claims BTC.".into(),
            "Preimage file is mode 600 under .rgbmvp/swaps/.".into(),
        ],
    })
}

fn fill_random(buf: &mut [u8]) -> Result<()> {
    use bitcoin::secp256k1::rand::rngs::OsRng;
    use bitcoin::secp256k1::rand::RngCore;
    OsRng.fill_bytes(buf);
    Ok(())
}

pub fn recompute_phase(s: &mut SwapSession) {
    if matches!(s.phase, SwapPhase::Refunded) {
        return;
    }
    let btc = s.btc_fund_txid.is_some();
    let lq = s.lq_fund_txid.is_some();
    let clq = s.lq_claim_txid.is_some();
    let cbtc = s.btc_claim_txid.is_some();
    s.phase = match (btc, lq, clq, cbtc) {
        (true, true, true, true) => SwapPhase::Done,
        (true, true, true, false) => SwapPhase::ClaimedLq,
        (true, true, false, false) => SwapPhase::FundedBoth,
        (true, false, _, _) => SwapPhase::FundedBtc,
        (false, true, _, _) => SwapPhase::FundedLq,
        _ => SwapPhase::Created,
    };
}
