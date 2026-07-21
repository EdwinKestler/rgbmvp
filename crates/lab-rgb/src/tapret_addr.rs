//! Encode an x-only output key as a Liquid taproot address (from spike-tapret).

use anyhow::{anyhow, Result};
use bech32::{segwit, Hrp};

pub const HRP_REGTEST: &str = "ert";
pub const HRP_LIQUIDV1: &str = "ex";
pub const HRP_TESTNET: &str = "tex";

pub fn encode_p2tr(network_hrp: &str, output_key: &[u8; 32]) -> Result<String> {
    let hrp = Hrp::parse(network_hrp).map_err(|e| anyhow!("bad hrp '{network_hrp}': {e}"))?;
    let fp = bech32::Fe32::try_from(1u8).expect("1 is a valid fe32");
    segwit::encode(hrp, fp, output_key.as_ref())
        .map_err(|e| anyhow!("bech32m encode failed: {e}"))
}
