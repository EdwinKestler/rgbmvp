//! IFA mint transitions + Elements vault backing check.
//!
//! Adapted from kaleidoswap/rgb-on-liquid-spike `mint.rs` (MIT OR Apache-2.0).

use amplify::confinement::Confined;
use anyhow::{Context, Result};
use rgbcore::bitcoin::OutPoint;
use rgbcore::commit_verify::CommitId;
use rgbcore::{
    BundleId, ContractId, GraphSeal, KnownTransition, OpId, Opout, Schema, Transition,
    TransitionBundle,
};
use rgbstd::contract::{AllocatedState, TransitionBuilder};
use rgbstd::{Amount, RevealedValue};
use schemata::OS_INFLATION;
use strict_encoding::FieldName;
use strict_types::TypeSystem;

/// Outcome of an IFA/BFA issuance (zero circulating supply; full allowance on gate).
#[derive(Debug, Clone)]
pub struct IfaIssuance {
    pub contract_id: ContractId,
    pub gate_seal_outpoint: OutPoint,
    pub max_supply: u64,
}

/// Build a mint (`TS_INFLATION` / `inflate`) under a caller-supplied schema (IFA or BFA).
#[allow(clippy::too_many_arguments)]
pub fn build_mint_with(
    schema: Schema,
    types: TypeSystem,
    contract_id: ContractId,
    gate_opid: OpId,
    gate_opout_no: u16,
    allowance_before: u64,
    mint_amount: u64,
    recipient_seal: OutPoint,
    new_gate_seal: Option<OutPoint>,
) -> Result<(BundleId, Transition)> {
    let remaining = allowance_before
        .checked_sub(mint_amount)
        .context("mint exceeds allowance")?;

    let mut builder =
        TransitionBuilder::named_transition(contract_id, schema, FieldName::from("inflate"), types)
            .map_err(|e| anyhow::anyhow!("TransitionBuilder::named_transition: {e:?}"))?;

    let input_opout = Opout::new(gate_opid, OS_INFLATION, gate_opout_no);
    builder = builder
        .add_input(
            input_opout,
            AllocatedState::Amount(RevealedValue::new(Amount::from(allowance_before))),
        )
        .map_err(|e| anyhow::anyhow!("add_input: {e:?}"))?;

    builder = builder
        .add_global_state(FieldName::from("issuedSupply"), Amount::from(mint_amount))
        .map_err(|e| anyhow::anyhow!("add issuedSupply: {e:?}"))?
        .add_metadata(FieldName::from("allowedInflation"), Amount::from(remaining))
        .map_err(|e| anyhow::anyhow!("add allowedInflation: {e:?}"))?;

    let recipient: GraphSeal =
        GraphSeal::with_blinding(recipient_seal.txid, recipient_seal.vout, 0u64);
    builder = builder
        .add_fungible_state(FieldName::from("assetOwner"), recipient, mint_amount)
        .map_err(|e| anyhow::anyhow!("add_fungible_state (recipient): {e:?}"))?;

    if remaining > 0 {
        let gate = new_gate_seal.context("remaining allowance > 0 requires a new gate seal")?;
        let gate: GraphSeal = GraphSeal::with_blinding(gate.txid, gate.vout, 1u64);
        builder = builder
            .add_fungible_state(FieldName::from("inflationAllowance"), gate, remaining)
            .map_err(|e| anyhow::anyhow!("add_fungible_state (gate): {e:?}"))?;
    }

    let transition = builder
        .complete_transition()
        .map_err(|e| anyhow::anyhow!("complete_transition: {e:?}"))?;
    let opid = transition.commit_id();

    let mut input_map = std::collections::BTreeMap::new();
    input_map.insert(input_opout, opid);
    let input_map =
        Confined::try_from(input_map).map_err(|e| anyhow::anyhow!("input_map: {e:?}"))?;
    let known_transitions =
        Confined::try_from(vec![KnownTransition::new(opid, transition.clone())])
            .map_err(|e| anyhow::anyhow!("known_transitions: {e:?}"))?;

    let bundle = TransitionBundle {
        input_map,
        known_transitions,
    };
    Ok((bundle.commit_id(), transition))
}

/// Sum explicit vault outputs matching `vault_spk` + `backing_asset`; require ≥ `required_amount`.
pub fn verify_backing(
    witness_tx: &elements::Transaction,
    vault_spk: &[u8],
    backing_asset: &elements::AssetId,
    required_amount: u64,
) -> Result<u64> {
    use elements::confidential::{Asset, Value};

    let mut locked = 0u64;
    for out in &witness_tx.output {
        if out.script_pubkey.as_bytes() != vault_spk {
            continue;
        }
        match (&out.asset, &out.value) {
            (Asset::Explicit(id), Value::Explicit(v)) if id == backing_asset => {
                locked = locked.saturating_add(*v);
            }
            (Asset::Explicit(_), _) | (_, Value::Explicit(_)) => {
                anyhow::bail!("vault output is partially blinded; the vault must be explicit");
            }
            _ => anyhow::bail!("vault output is blinded; the vault must be explicit"),
        }
    }

    if locked < required_amount {
        anyhow::bail!(
            "under-backed mint: locked {locked} of backing asset, mint requires {required_amount}"
        );
    }
    Ok(locked)
}
