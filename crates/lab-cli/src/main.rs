//! rgbmvp CLI — Phase 0 + P0 (network, LWK wallet, RGB issue/transfer/verify, labd).

use std::fs;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use lab_core::Config;
use lab_rgb::storage::RgbStore;
use lab_rgb::swap::{self, SwapStore};
use lab_rgb::{
    issue_nia, plan_transfer, verify_against_witness, IssueRequest, DEMO_INTERNAL_XONLY_HEX,
};
use lab_rgb::htlc;

#[derive(Parser, Debug)]
#[command(
    name = "rgbmvp",
    about = "RGB Liquid Testnet Lab — CLI (P0: RGB issue/transfer/verify)",
    version
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    Net {
        #[command(subcommand)]
        cmd: NetCmd,
    },
    Wallet {
        #[command(subcommand)]
        cmd: WalletCmd,
    },
    /// RGB-on-Liquid (NIA) commands
    Rgb {
        #[command(subcommand)]
        cmd: RgbCmd,
    },
    /// Bitcoin testnet wallet (P1 twin-swap leg)
    Btc {
        #[command(subcommand)]
        cmd: BtcCmd,
    },
    /// BTC ↔ Liquid RGB atomic swap (HTLC)
    Swap {
        #[command(subcommand)]
        cmd: SwapCmd,
    },
    /// Serve labd HTTP API + web verifier (static)
    Serve {
        #[arg(long, env = "LABD_BIND")]
        bind: Option<String>,
    },
    /// P2 Simplicity covenants (C0: RGB-anchor seal)
    Covenant {
        #[command(subcommand)]
        cmd: CovenantCmd,
    },
    /// P2 C3 BFA (backed fungible asset) issue / mint plan / audit
    Bfa {
        #[command(subcommand)]
        cmd: BfaCmd,
    },
    ApiRoot,
}

#[derive(Subcommand, Debug)]
enum BfaCmd {
    /// Issue BFA genesis (prints contract_id + terms echo)
    Issue {
        #[arg(long, default_value = "LiquidRgbUSD")]
        name: String,
        #[arg(long, default_value = "LRUSD")]
        ticker: String,
        #[arg(long, default_value_t = 1_000_000)]
        max_supply: u64,
        /// Gate seal outpoint txid:vout
        #[arg(long)]
        gate_seal: String,
        /// Canonical terms: elements-backing:v1;vault=…;asset=…;rate=n/d
        #[arg(long)]
        backing: String,
        #[arg(long, default_value = "elements-regtest")]
        chain: String,
    },
    /// Plan a BFA mint transition (tapret address + opid JSON)
    MintPlan {
        #[arg(long, default_value = "LiquidRgbUSD")]
        name: String,
        #[arg(long, default_value = "LRUSD")]
        ticker: String,
        #[arg(long, default_value_t = 1_000_000)]
        max_supply: u64,
        #[arg(long)]
        backing: String,
        #[arg(long)]
        genesis_gate: String,
        #[arg(long)]
        gate_seal: String,
        #[arg(long)]
        mint: u64,
        #[arg(long)]
        recipient_seal: String,
        #[arg(long)]
        new_gate_seal: String,
        #[arg(long)]
        consume_opid: Option<String>,
        #[arg(long)]
        allowance: Option<u64>,
        #[arg(long, default_value = lab_rgb::DEMO_INTERNAL_XONLY_HEX)]
        internal_key: String,
        #[arg(long, default_value_t = 12_648_430)]
        entropy: u64,
        #[arg(long, default_value = "elements-regtest")]
        chain: String,
    },
    /// Full-history audit from JSON history file
    Audit {
        #[arg(long)]
        history: PathBuf,
        /// Fetch missing witness txs via regtest_simplicity.sh cli
        #[arg(long, default_value_t = true)]
        fetch_rpc: bool,
    },
    /// End-to-end C3 demo script
    Demo {
        #[arg(long, default_value = "scripts/demo_c3_bfa_audit.sh")]
        script: PathBuf,
    },
}

#[derive(Subcommand, Debug)]
enum CovenantCmd {
    /// Compile C0 rgb_anchor program → taproot address (leaf 0xbe)
    Address {
        /// SHA256(preimage) as 32-byte hex (param EXPECTED_HASH)
        #[arg(long)]
        hash: String,
        #[arg(long)]
        program: Option<PathBuf>,
    },
    /// End-to-end C0 demo on Elements Simplicity regtest
    Demo {
        #[arg(long, default_value = "scripts/demo_c0_simplicity.sh")]
        script: PathBuf,
    },
    /// End-to-end C1 mint-gate demo (vault + recursion)
    DemoC1 {
        #[arg(long, default_value = "scripts/demo_c1_mint_gate.sh")]
        script: PathBuf,
    },
}

#[derive(Subcommand, Debug)]
enum SwapCmd {
    /// Create swap session (preimage + dual HTLCs)
    Init {
        #[arg(long)]
        id: String,
        #[arg(long, default_value_t = 6)]
        csv_delay: u32,
        #[arg(long, default_value = "btc-alice")]
        alice_btc: String,
        #[arg(long, default_value = "bob")]
        bob_lq: String,
        #[arg(long)]
        btc_contract: Option<String>,
        #[arg(long)]
        lq_contract: Option<String>,
    },
    Status {
        #[arg(long)]
        id: String,
    },
    /// Fund BTC HTLC from alice BTC wallet
    FundBtc {
        #[arg(long)]
        id: String,
        #[arg(long, default_value_t = 10000)]
        amount_sats: u64,
        #[arg(long, default_value_t = 800)]
        fee_sats: u64,
    },
    /// Fund Liquid HTLC from bob LWK wallet
    FundLq {
        #[arg(long)]
        id: String,
        #[arg(long, default_value_t = 5000)]
        amount_sats: u64,
    },
    /// Alice claims Liquid HTLC (reveals preimage)
    ClaimLq {
        #[arg(long)]
        id: String,
        #[arg(long, default_value_t = 300)]
        fee_sats: u64,
    },
    /// Bob claims BTC HTLC using preimage
    ClaimBtc {
        #[arg(long)]
        id: String,
        #[arg(long, default_value_t = 500)]
        fee_sats: u64,
    },
    /// Alice refunds BTC HTLC after CSV delay (no preimage)
    RefundBtc {
        #[arg(long)]
        id: String,
        #[arg(long, default_value_t = 500)]
        fee_sats: u64,
    },
    /// Bob refunds Liquid HTLC after CSV delay (no preimage)
    RefundLq {
        #[arg(long)]
        id: String,
        #[arg(long, default_value_t = 300)]
        fee_sats: u64,
    },
    HtlcAddress {
        #[arg(long)]
        hash_hex: String,
        #[arg(long, default_value_t = 6)]
        csv_delay: u32,
        #[arg(long, default_value = "bob-claimer")]
        claimer: String,
        #[arg(long, default_value = "alice-refund")]
        refund: String,
    },
}

#[derive(Subcommand, Debug)]
enum BtcCmd {
    /// Probe Bitcoin testnet Esplora (+ btc-alice balance if imported)
    Status,
    /// Import BTC_TESTNET_WIF from .env as wallet btc-alice
    ImportEnv {
        #[arg(long)]
        force: bool,
    },
    /// Import a WIF into a named BTC wallet
    ImportWif {
        #[arg(long, default_value = "btc-alice")]
        name: String,
        #[arg(long)]
        wif: String,
        #[arg(long)]
        expect_address: Option<String>,
        #[arg(long)]
        force: bool,
    },
    Address {
        #[arg(long, default_value = "btc-alice")]
        name: String,
    },
    Balance {
        #[arg(long, default_value = "btc-alice")]
        name: String,
    },
    Utxos {
        #[arg(long, default_value = "btc-alice")]
        name: String,
    },
}

#[derive(Subcommand, Debug)]
enum NetCmd {
    Status,
}

#[derive(Subcommand, Debug)]
enum WalletCmd {
    /// Random ad-hoc wallet (prefer bootstrap-testnet for tests)
    Create {
        #[arg(long, default_value = "default")]
        name: String,
        #[arg(long)]
        force: bool,
    },
    /// Import a mnemonic into a named reusable wallet
    Import {
        #[arg(long)]
        name: String,
        #[arg(long)]
        mnemonic: Option<String>,
        #[arg(long)]
        mnemonic_file: Option<PathBuf>,
        #[arg(long)]
        role: Option<String>,
        #[arg(long)]
        force: bool,
    },
    /// Import alice/bob/carol/maker from fixtures/testnet_wallets.json
    BootstrapTestnet {
        #[arg(long, default_value = "fixtures/testnet_wallets.json")]
        fixture: PathBuf,
        #[arg(long)]
        force: bool,
    },
    /// List local wallets
    List {
        #[arg(long)]
        sync: bool,
    },
    /// Write address registry (no secrets)
    Registry,
    Address {
        #[arg(long, default_value = "default")]
        name: String,
        #[arg(long)]
        index: Option<u32>,
    },
    Balance {
        #[arg(long, default_value = "default")]
        name: String,
    },
    Utxos {
        #[arg(long, default_value = "default")]
        name: String,
    },
    /// Send L-BTC between wallets or to an address (testnet rebalance)
    Send {
        #[arg(long)]
        from: String,
        #[arg(long)]
        to: Option<String>,
        #[arg(long)]
        to_address: Option<String>,
        #[arg(long)]
        amount_sats: u64,
    },
}

#[derive(Subcommand, Debug)]
enum RgbCmd {
    /// Issue RGB20 (NIA) on Liquid Testnet using a wallet L-BTC UTXO as seal
    Issue {
        /// liquid-testnet | bitcoin-testnet
        #[arg(long, default_value = "liquid-testnet")]
        chain: String,
        /// Liquid LWK name or BTC wallet name (btc-alice)
        #[arg(long, default_value = "alice")]
        wallet: String,
        #[arg(long, default_value = "Test RGB")]
        name: String,
        #[arg(long, default_value = "tRGB")]
        ticker: String,
        #[arg(long, default_value_t = 1_000_000)]
        supply: u64,
        /// Optional seal outpoint; default = largest UTXO on that chain
        #[arg(long)]
        seal: Option<String>,
    },
    /// Build receive invoice JSON (seal intent + contract)
    Invoice {
        #[arg(long)]
        contract: String,
        #[arg(long, default_value = "bob")]
        wallet: String,
        #[arg(long, default_value_t = 1000)]
        amount: u64,
    },
    /// Build transfer plan (MPC + tapret); optionally broadcast commitment tx
    Transfer {
        #[arg(long, default_value = "liquid-testnet")]
        chain: String,
        #[arg(long)]
        contract: String,
        #[arg(long, default_value = "alice")]
        wallet: String,
        #[arg(long, default_value_t = 600_000)]
        amount: u64,
        /// Optional Bob L-BTC address (Liquid confidential/unconfidential)
        #[arg(long)]
        bob_address: Option<String>,
        #[arg(long, default_value_t = 1000)]
        bob_sats: u64,
        #[arg(long, default_value_t = 500)]
        commitment_sats: u64,
        /// Broadcast the seal-closing Liquid tx via LWK
        #[arg(long)]
        broadcast: bool,
        #[arg(long, default_value_t = 42)]
        entropy: u64,
    },
    /// Verify transfer plan against a Liquid witness txid (Esplora)
    Verify {
        #[arg(long)]
        plan: String,
        #[arg(long)]
        txid: String,
        #[arg(long)]
        proof_id: Option<String>,
        /// Override chain for witness fetch: liquid-testnet | bitcoin-testnet
        #[arg(long)]
        chain: Option<String>,
    },
    /// Store an opaque consignment blob (file) for exchange
    Consign {
        #[command(subcommand)]
        cmd: ConsignCmd,
    },
}

#[derive(Subcommand, Debug)]
enum ConsignCmd {
    Put {
        #[arg(long)]
        id: String,
        #[arg(long)]
        file: PathBuf,
    },
    Get {
        #[arg(long)]
        id: String,
        #[arg(long)]
        out: PathBuf,
    },
}

fn main() {
    if let Err(e) = run() {
        let err = serde_json::json!({
            "status": "error",
            "error": format!("{e:#}"),
        });
        eprintln!(
            "{}",
            serde_json::to_string_pretty(&err).unwrap_or_else(|_| e.to_string())
        );
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse();
    let cfg = Config::load()?;
    cfg.ensure_dirs()?;
    let store = RgbStore::new(&cfg.data_dir);
    store.ensure()?;

    match cli.command {
        Commands::Net { cmd: NetCmd::Status } => {
            let report = lab_chain::network_status(&cfg)?;
            println!(
                "{}",
                serde_json::to_string_pretty(&lab_api::health_json(&report))?
            );
            if report.status != "ready" {
                std::process::exit(2);
            }
        }
        Commands::Wallet { cmd } => match cmd {
            WalletCmd::Create { name, force } => {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&lab_chain::wallet_create(&cfg, &name, force)?)?
                );
            }
            WalletCmd::Import {
                name,
                mnemonic,
                mnemonic_file,
                role,
                force,
            } => {
                let phrase = if let Some(p) = mnemonic {
                    p
                } else if let Some(f) = mnemonic_file {
                    std::fs::read_to_string(&f)?.trim().to_string()
                } else {
                    anyhow::bail!("provide --mnemonic or --mnemonic-file");
                };
                println!(
                    "{}",
                    serde_json::to_string_pretty(&lab_chain::wallet_import(
                        &cfg,
                        &name,
                        &phrase,
                        force,
                        role.as_deref()
                    )?)?
                );
            }
            WalletCmd::BootstrapTestnet { fixture, force } => {
                let path = if fixture.exists() {
                    fixture
                } else {
                    PathBuf::from("fixtures/testnet_wallets.json")
                };
                println!(
                    "{}",
                    serde_json::to_string_pretty(&lab_chain::wallet_bootstrap_fixtures(
                        &cfg, &path, force
                    )?)?
                );
            }
            WalletCmd::List { sync } => {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&lab_chain::wallet_list(&cfg, sync)?)?
                );
            }
            WalletCmd::Registry => {
                let p = lab_chain::write_wallet_registry(&cfg)?;
                println!(
                    "{}",
                    serde_json::to_string_pretty(&serde_json::json!({
                        "status": "ok",
                        "registry": p.display().to_string(),
                    }))?
                );
            }
            WalletCmd::Address { name, index } => {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&lab_chain::wallet_address(
                        &cfg, &name, index
                    )?)?
                );
            }
            WalletCmd::Balance { name } => {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&lab_chain::wallet_balance(&cfg, &name)?)?
                );
            }
            WalletCmd::Utxos { name } => {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&lab_chain::wallet_utxos(&cfg, &name)?)?
                );
            }
            WalletCmd::Send {
                from,
                to,
                to_address,
                amount_sats,
            } => {
                let dest = if let Some(addr) = to_address {
                    addr
                } else if let Some(w) = to {
                    lab_chain::wallet_receive_address(&cfg, &w)?
                } else {
                    anyhow::bail!("provide --to <wallet> or --to-address");
                };
                println!(
                    "{}",
                    serde_json::to_string_pretty(&lab_chain::send_lbtc(
                        &cfg,
                        &from,
                        &dest,
                        amount_sats
                    )?)?
                );
            }
        },
        Commands::Rgb { cmd } => match cmd {
            RgbCmd::Issue {
                chain,
                wallet,
                name,
                ticker,
                supply,
                seal,
            } => {
                let seal = match seal {
                    Some(s) => s,
                    None if chain.starts_with("bitcoin") || chain == "testnet" || chain == "testnet3" => {
                        let btc = lab_btc::BtcConfig::from_env();
                        lab_btc::pick_largest_utxo(&cfg, &btc, &wallet)?.outpoint
                    }
                    None => lab_chain::pick_lbtc_seal(&cfg, &wallet)?.outpoint,
                };
                let issue = issue_nia(&IssueRequest {
                    name,
                    ticker,
                    supply,
                    seal: seal.clone(),
                    chain: chain.clone(),
                })?;
                let path = store.save_issue(&issue)?;
                let out = serde_json::json!({
                    "status": "issued",
                    "issue": issue,
                    "stored": path.display().to_string(),
                    "note": "Genesis is off-chain; seal UTXO must be closed by a transfer witness tx."
                });
                println!("{}", serde_json::to_string_pretty(&out)?);
            }
            RgbCmd::Invoice {
                contract,
                wallet,
                amount,
            } => {
                let issue = store.load_issue(&contract).with_context(|| {
                    format!("contract not found in store: {contract} (run rgb issue first)")
                })?;
                let addr = lab_chain::wallet_address(&cfg, &wallet, None)?;
                let inv = serde_json::json!({
                    "type": "rgbmvp-invoice-v1",
                    "network": "liquid-testnet",
                    "contract_id": issue.contract_id,
                    "ticker": issue.ticker,
                    "amount": amount,
                    "receive_address": addr.address,
                    "note": "P0 invoice: Bob should fund/use a seal UTXO; amount is RGB units."
                });
                println!("{}", serde_json::to_string_pretty(&inv)?);
            }
            RgbCmd::Transfer {
                chain,
                contract,
                wallet,
                amount,
                bob_address,
                bob_sats,
                commitment_sats,
                broadcast,
                entropy,
            } => {
                let issue = store.load_issue(&contract)?;
                let chain = if chain != "liquid-testnet" {
                    chain
                } else if issue.chain_net.starts_with("bitcoin") {
                    issue.chain_net.clone()
                } else {
                    chain
                };
                let plan = plan_transfer(
                    &issue.contract_id,
                    issue.supply,
                    amount,
                    &issue.seal,
                    &format!("bob-{}", issue.contract_id),
                    &format!("change-{}", issue.contract_id),
                    DEMO_INTERNAL_XONLY_HEX,
                    entropy,
                    &issue.ticker,
                    &chain,
                )?;
                let plan_id = format!(
                    "{}-{}",
                    issue.ticker,
                    &plan.bundle_id_hex[..16.min(plan.bundle_id_hex.len())]
                );
                let plan_path = store.save_transfer(&plan_id, &plan)?;

                let mut out = serde_json::json!({
                    "status": "planned",
                    "plan_id": plan_id,
                    "plan_path": plan_path.display().to_string(),
                    "plan": plan,
                });

                if broadcast {
                    let is_btc = chain.starts_with("bitcoin") || chain.contains("testnet3");
                    let bc_val = if is_btc {
                        let btc = lab_btc::BtcConfig::from_env();
                        // resolve seal value from utxo list
                        let utxos = lab_btc::utxos(&cfg, &btc, &wallet)?;
                        let seal_val = utxos
                            .iter()
                            .find(|u| u.outpoint == issue.seal)
                            .map(|u| u.value_sats)
                            .context("seal UTXO not found in btc wallet")?;
                        let fee = 800u64;
                        let bc = lab_btc::broadcast_commitment_tx(
                            &cfg,
                            &btc,
                            &wallet,
                            &issue.seal,
                            seal_val,
                            &plan.tapret_address,
                            commitment_sats,
                            fee,
                        )?;
                        serde_json::to_value(bc)?
                    } else {
                        let bc = lab_chain::broadcast_commitment_tx(
                            &cfg,
                            &wallet,
                            &issue.seal,
                            &plan.tapret_address,
                            bob_address.as_deref(),
                            commitment_sats,
                            bob_sats,
                        )?;
                        serde_json::to_value(bc)?
                    };
                    let meta = serde_json::json!({
                        "plan": plan,
                        "broadcast": bc_val,
                    });
                    let meta_path = cfg
                        .data_dir
                        .join("rgb/transfers")
                        .join(format!("{plan_id}.broadcast.json"));
                    fs::write(&meta_path, serde_json::to_vec_pretty(&meta)?)?;
                    out["status"] = serde_json::json!("broadcast");
                    out["broadcast"] = bc_val;
                    out["broadcast_meta"] = serde_json::json!(meta_path.display().to_string());
                }
                println!("{}", serde_json::to_string_pretty(&out)?);
            }
            RgbCmd::Verify {
                plan,
                txid,
                proof_id,
                chain,
            } => {
                let plan_obj = store.load_transfer(&plan)?;
                let chain = chain.unwrap_or_else(|| plan_obj.chain_net.clone());
                let (witness, explorer) = if chain.starts_with("bitcoin") {
                    let btc = lab_btc::BtcConfig::from_env();
                    (
                        lab_btc::fetch_witness_for_rgb(&btc, &txid)?,
                        btc.explorer_base.clone(),
                    )
                } else {
                    let api = lab_chain::esplora_api_base(&cfg);
                    (
                        lab_chain::fetch_witness_esplora(&api, &txid)?,
                        cfg.explorer_base.clone(),
                    )
                };
                let result = verify_against_witness(&plan_obj, &witness, &explorer)?;
                let pid =
                    proof_id.unwrap_or_else(|| format!("proof-{}", &txid[..16.min(txid.len())]));
                let path = store.save_proof(&pid, &result)?;
                let out = serde_json::json!({
                    "proof_id": pid,
                    "stored": path.display().to_string(),
                    "result": result,
                });
                println!("{}", serde_json::to_string_pretty(&out)?);
                if result.status != "valid" {
                    std::process::exit(2);
                }
            }
            RgbCmd::Consign { cmd } => match cmd {
                ConsignCmd::Put { id, file } => {
                    let bytes = fs::read(&file).with_context(|| format!("read {}", file.display()))?;
                    let path = store.save_consignment_blob(&id, &bytes)?;
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&serde_json::json!({
                            "status": "stored",
                            "id": id,
                            "bytes": bytes.len(),
                            "path": path.display().to_string(),
                        }))?
                    );
                }
                ConsignCmd::Get { id, out } => {
                    let src = store.root().join("consignments").join(format!("{id}.bin"));
                    fs::copy(&src, &out)
                        .with_context(|| format!("copy {} -> {}", src.display(), out.display()))?;
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&serde_json::json!({
                            "status": "exported",
                            "id": id,
                            "out": out.display().to_string(),
                        }))?
                    );
                }
            },
        },
        Commands::Swap { cmd } => {
            let store = SwapStore::new(&cfg.data_dir);
            match cmd {
                SwapCmd::Init {
                    id,
                    csv_delay,
                    alice_btc,
                    bob_lq,
                    btc_contract,
                    lq_contract,
                } => {
                    let session = swap::init_swap(
                        &id,
                        csv_delay,
                        &alice_btc,
                        &bob_lq,
                        btc_contract,
                        lq_contract,
                    )?;
                    let path = store.save(&session)?;
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&serde_json::json!({
                            "status": "created",
                            "stored": path.display().to_string(),
                            "session": session,
                            "next": [
                                "rgbmvp swap fund-btc --id …",
                                "rgbmvp swap fund-lq --id …",
                                "rgbmvp swap claim-lq --id …  # Alice reveals preimage",
                                "rgbmvp swap claim-btc --id … # Bob claims BTC",
                            ]
                        }))?
                    );
                }
                SwapCmd::Status { id } => {
                    let s = store.load(&id)?;
                    println!("{}", serde_json::to_string_pretty(&s)?);
                }
                SwapCmd::FundBtc {
                    id,
                    amount_sats,
                    fee_sats,
                } => {
                    let mut s = store.load(&id)?;
                    let btc = lab_btc::BtcConfig::from_env();
                    let bc = lab_btc::fund_address(
                        &cfg,
                        &btc,
                        &s.alice_btc_wallet,
                        &s.htlc_btc.address_btc,
                        amount_sats,
                        fee_sats,
                    )?;
                    s.btc_fund_txid = Some(bc.txid.clone());
                    s.btc_fund_vout = Some(0);
                    s.btc_fund_sats = Some(amount_sats);
                    swap::recompute_phase(&mut s);
                    store.save(&s)?;
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&serde_json::json!({
                            "status": "funded_btc",
                            "phase": s.phase,
                            "broadcast": bc,
                            "htlc_address": s.htlc_btc.address_btc,
                        }))?
                    );
                }
                SwapCmd::FundLq { id, amount_sats } => {
                    let mut s = store.load(&id)?;
                    let bc = lab_chain::send_lbtc(
                        &cfg,
                        &s.bob_lq_wallet,
                        &s.htlc_lq.address_liquid_unconf,
                        amount_sats,
                    )?;
                    s.lq_fund_txid = Some(bc.txid.clone());
                    s.lq_fund_vout = Some(0);
                    s.lq_fund_sats = Some(amount_sats);
                    swap::recompute_phase(&mut s);
                    store.save(&s)?;
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&serde_json::json!({
                            "status": "funded_lq",
                            "phase": s.phase,
                            "broadcast": bc,
                            "htlc_address": s.htlc_lq.address_liquid_unconf,
                        }))?
                    );
                }
                SwapCmd::ClaimLq { id, fee_sats } => {
                    let mut s = store.load(&id)?;
                    let amount = s.lq_fund_sats.context("lq not funded (run fund-lq)")?;
                    let (txid, vout, value) = lab_chain::find_address_utxo(
                        &cfg,
                        &s.htlc_lq.address_liquid_unconf,
                        amount.saturating_sub(1),
                    )?;
                    s.lq_fund_txid = Some(txid.clone());
                    s.lq_fund_vout = Some(vout);
                    s.lq_fund_sats = Some(value);

                    let preimage = hex::decode(&s.preimage_hex)?;
                    let (claimer_sk, _) = htlc::demo_keypair(&s.htlc_lq.claimer_label)?;
                    let ws = hex::decode(&s.htlc_lq.witness_script_hex)?;
                    use bitcoin::key::{CompressedPublicKey, Secp256k1};
                    use bitcoin::{Address, Network};
                    let secp = Secp256k1::new();
                    let pk = bitcoin::secp256k1::PublicKey::from_secret_key(&secp, &claimer_sk);
                    let compressed = CompressedPublicKey(pk);
                    let dest = Address::p2wpkh(&compressed, Network::Testnet);
                    let dest_spk = dest.script_pubkey();
                    let policy = "144c654344aa716d6f3abcc1ca90e5641e4e2a7f633bc09fe3baf64585819a49";
                    let out_sats = value.saturating_sub(fee_sats);
                    let raw = htlc::build_htlc_spend_liquid(
                        &txid,
                        vout,
                        value,
                        out_sats,
                        fee_sats,
                        dest_spk.as_bytes(),
                        policy,
                        &ws,
                        htlc::HtlcSpend::Claim {
                            preimage: &preimage,
                        },
                        s.csv_delay,
                        &claimer_sk,
                    )?;
                    let claim_txid = lab_chain::broadcast_raw_hex(&cfg, &raw)?;
                    s.lq_claim_txid = Some(claim_txid.clone());
                    swap::recompute_phase(&mut s);
                    store.save(&s)?;
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&serde_json::json!({
                            "status": "claimed_lq",
                            "phase": s.phase,
                            "txid": claim_txid,
                            "explorer": format!("{}/tx/{}", cfg.explorer_base, claim_txid),
                            "preimage_published": true,
                            "note": "Preimage is public on Liquid; Bob can claim BTC.",
                        }))?
                    );
                }
                SwapCmd::ClaimBtc { id, fee_sats } => {
                    let mut s = store.load(&id)?;
                    let btc = lab_btc::BtcConfig::from_env();
                    let amount = s.btc_fund_sats.context("btc_fund_sats")?;
                    let utxo = lab_btc::find_htlc_utxo(
                        &btc,
                        &s.htlc_btc.address_btc,
                        amount.saturating_sub(1),
                    )?;
                    let preimage = hex::decode(&s.preimage_hex)?;
                    let (claimer_sk, _) = htlc::demo_keypair(&s.htlc_btc.claimer_label)?;
                    let ws = hex::decode(&s.htlc_btc.witness_script_hex)?;
                    use bitcoin::key::{CompressedPublicKey, Secp256k1};
                    use bitcoin::{Address, Network};
                    let secp = Secp256k1::new();
                    let pk = bitcoin::secp256k1::PublicKey::from_secret_key(&secp, &claimer_sk);
                    let compressed = CompressedPublicKey(pk);
                    let dest = Address::p2wpkh(&compressed, Network::Testnet);
                    let dest_spk = dest.script_pubkey();
                    let out_sats = utxo.value_sats.saturating_sub(fee_sats);
                    let raw = htlc::build_htlc_spend_btc(
                        &utxo.txid,
                        utxo.vout,
                        utxo.value_sats,
                        out_sats,
                        dest_spk.as_bytes(),
                        &ws,
                        htlc::HtlcSpend::Claim {
                            preimage: &preimage,
                        },
                        s.csv_delay,
                        &claimer_sk,
                    )?;
                    let txid = lab_btc::broadcast_raw(&btc, &raw)?;
                    s.btc_claim_txid = Some(txid.clone());
                    swap::recompute_phase(&mut s);
                    store.save(&s)?;
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&serde_json::json!({
                            "status": "claimed_btc",
                            "phase": s.phase,
                            "txid": txid,
                            "explorer": format!("{}/tx/{}", btc.explorer_base, txid),
                            "dest": dest.to_string(),
                        }))?
                    );
                }
                SwapCmd::RefundBtc { id, fee_sats } => {
                    let mut s = store.load(&id)?;
                    if s.btc_claim_txid.is_some() {
                        anyhow::bail!("BTC already claimed; cannot refund");
                    }
                    let btc = lab_btc::BtcConfig::from_env();
                    let amount = s.btc_fund_sats.context("btc not funded")?;
                    let utxo = lab_btc::find_htlc_utxo(
                        &btc,
                        &s.htlc_btc.address_btc,
                        amount.saturating_sub(1),
                    )?;
                    let (refund_sk, _) = htlc::demo_keypair(&s.htlc_btc.refund_label)?;
                    let ws = hex::decode(&s.htlc_btc.witness_script_hex)?;
                    use bitcoin::key::{CompressedPublicKey, Secp256k1};
                    use bitcoin::{Address, Network};
                    let secp = Secp256k1::new();
                    let pk = bitcoin::secp256k1::PublicKey::from_secret_key(&secp, &refund_sk);
                    let compressed = CompressedPublicKey(pk);
                    let dest = Address::p2wpkh(&compressed, Network::Testnet);
                    let out_sats = utxo.value_sats.saturating_sub(fee_sats);
                    let raw = htlc::build_htlc_spend_btc(
                        &utxo.txid,
                        utxo.vout,
                        utxo.value_sats,
                        out_sats,
                        dest.script_pubkey().as_bytes(),
                        &ws,
                        htlc::HtlcSpend::Refund,
                        s.csv_delay,
                        &refund_sk,
                    )?;
                    let txid = lab_btc::broadcast_raw(&btc, &raw)?;
                    s.notes.push(format!("btc_refund_txid={txid}"));
                    s.phase = lab_rgb::swap::SwapPhase::Refunded;
                    store.save(&s)?;
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&serde_json::json!({
                            "status": "refunded_btc",
                            "phase": s.phase,
                            "txid": txid,
                            "explorer": format!("{}/tx/{}", btc.explorer_base, txid),
                            "dest": dest.to_string(),
                            "note": "Requires CSV maturity (nSequence = csv_delay blocks) since fund.",
                        }))?
                    );
                }
                SwapCmd::RefundLq { id, fee_sats } => {
                    let mut s = store.load(&id)?;
                    if s.lq_claim_txid.is_some() {
                        anyhow::bail!("Liquid already claimed; cannot refund");
                    }
                    let amount = s.lq_fund_sats.context("lq not funded")?;
                    let (txid, vout, value) = lab_chain::find_address_utxo(
                        &cfg,
                        &s.htlc_lq.address_liquid_unconf,
                        amount.saturating_sub(1),
                    )?;
                    let (refund_sk, _) = htlc::demo_keypair(&s.htlc_lq.refund_label)?;
                    let ws = hex::decode(&s.htlc_lq.witness_script_hex)?;
                    use bitcoin::key::{CompressedPublicKey, Secp256k1};
                    use bitcoin::{Address, Network};
                    let secp = Secp256k1::new();
                    let pk = bitcoin::secp256k1::PublicKey::from_secret_key(&secp, &refund_sk);
                    let compressed = CompressedPublicKey(pk);
                    let dest = Address::p2wpkh(&compressed, Network::Testnet);
                    let policy = "144c654344aa716d6f3abcc1ca90e5641e4e2a7f633bc09fe3baf64585819a49";
                    let out_sats = value.saturating_sub(fee_sats);
                    let raw = htlc::build_htlc_spend_liquid(
                        &txid,
                        vout,
                        value,
                        out_sats,
                        fee_sats,
                        dest.script_pubkey().as_bytes(),
                        policy,
                        &ws,
                        htlc::HtlcSpend::Refund,
                        s.csv_delay,
                        &refund_sk,
                    )?;
                    let claim_txid = lab_chain::broadcast_raw_hex(&cfg, &raw)?;
                    s.notes.push(format!("lq_refund_txid={claim_txid}"));
                    s.phase = lab_rgb::swap::SwapPhase::Refunded;
                    store.save(&s)?;
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&serde_json::json!({
                            "status": "refunded_lq",
                            "phase": s.phase,
                            "txid": claim_txid,
                            "explorer": format!("{}/tx/{}", cfg.explorer_base, claim_txid),
                            "note": "Requires CSV maturity since fund.",
                        }))?
                    );
                }
                SwapCmd::HtlcAddress {
                    hash_hex,
                    csv_delay,
                    claimer,
                    refund,
                } => {
                    let mut h = [0u8; 32];
                    let b = hex::decode(hash_hex.trim())?;
                    if b.len() != 32 {
                        anyhow::bail!("hash must be 32 bytes");
                    }
                    h.copy_from_slice(&b);
                    let info = htlc::build_htlc_addresses(&h, &claimer, &refund, csv_delay)?;
                    println!("{}", serde_json::to_string_pretty(&info)?);
                }
            }
        }
        Commands::Btc { cmd } => {
            let btc = lab_btc::BtcConfig::from_env();
            match cmd {
                BtcCmd::Status => {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&lab_btc::network_status(&cfg, &btc)?)?
                    );
                }
                BtcCmd::ImportEnv { force } => {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&lab_btc::import_from_env(
                            &cfg, &btc, force
                        )?)?
                    );
                }
                BtcCmd::ImportWif {
                    name,
                    wif,
                    expect_address,
                    force,
                } => {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&lab_btc::import_wif(
                            &cfg,
                            &btc,
                            &name,
                            &wif,
                            expect_address.as_deref(),
                            force
                        )?)?
                    );
                }
                BtcCmd::Address { name } => {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&lab_btc::load_wallet_address(
                            &cfg, &btc, &name
                        )?)?
                    );
                }
                BtcCmd::Balance { name } => {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&lab_btc::balance(&cfg, &btc, &name)?)?
                    );
                }
                BtcCmd::Utxos { name } => {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&lab_btc::utxos(&cfg, &btc, &name)?)?
                    );
                }
            }
        }
        Commands::Serve { bind } => {
            let bind = bind.unwrap_or_else(|| cfg.labd_bind.clone());
            serve_labd(&cfg, &bind)?;
        }
        Commands::Covenant { cmd } => match cmd {
            CovenantCmd::Address { hash, program } => {
                let path = program.unwrap_or_else(lab_simplicity::resolve_rgb_anchor_program);
                let src = fs::read_to_string(&path)
                    .with_context(|| format!("read {}", path.display()))?;
                let args = lab_simplicity::args_expected_hash_json(&hash)?;
                let compiled = lab_simplicity::compile_src(&src, &args)?;
                let info = lab_simplicity::address_info(&compiled)?;
                println!("{}", serde_json::to_string_pretty(&info)?);
            }
            CovenantCmd::Demo { script } => {
                anyhow::ensure!(
                    script.is_file(),
                    "demo script missing: {} (run from repo root)",
                    script.display()
                );
                let status = std::process::Command::new("bash")
                    .arg(&script)
                    .status()
                    .with_context(|| format!("run {}", script.display()))?;
                if !status.success() {
                    anyhow::bail!("demo exited with {status}");
                }
            }
            CovenantCmd::DemoC1 { script } => {
                anyhow::ensure!(
                    script.is_file(),
                    "demo script missing: {} (run from repo root)",
                    script.display()
                );
                let status = std::process::Command::new("bash")
                    .arg(&script)
                    .status()
                    .with_context(|| format!("run {}", script.display()))?;
                if !status.success() {
                    anyhow::bail!("demo exited with {status}");
                }
            }
        },
        Commands::Bfa { cmd } => match cmd {
            BfaCmd::Issue {
                name,
                ticker,
                max_supply,
                gate_seal,
                backing,
                chain,
            } => {
                let json = lab_rgb::bfa::issue_json(
                    &name, &ticker, max_supply, &gate_seal, &backing, &chain,
                )?;
                println!("{}", serde_json::to_string_pretty(&json)?);
            }
            BfaCmd::MintPlan {
                name,
                ticker,
                max_supply,
                backing,
                genesis_gate,
                gate_seal,
                mint,
                recipient_seal,
                new_gate_seal,
                consume_opid,
                allowance,
                internal_key,
                entropy,
                chain,
            } => {
                let json = lab_rgb::bfa::plan_mint_json(
                    &name,
                    &ticker,
                    max_supply,
                    &backing,
                    &genesis_gate,
                    &gate_seal,
                    mint,
                    &recipient_seal,
                    &new_gate_seal,
                    consume_opid.as_deref(),
                    allowance,
                    &internal_key,
                    entropy,
                    &chain,
                )?;
                println!("{}", serde_json::to_string_pretty(&json)?);
            }
            BfaCmd::Audit {
                history,
                fetch_rpc,
            } => {
                let s = fs::read_to_string(&history)
                    .with_context(|| format!("read {}", history.display()))?;
                let hist: lab_rgb::bfa::BfaHistory = serde_json::from_str(&s)?;
                let fetch = |txid: &str| -> Result<String> {
                    if !fetch_rpc {
                        anyhow::bail!("no witness_tx_hex for {txid} and --fetch-rpc disabled");
                    }
                    let out = std::process::Command::new("./scripts/regtest_simplicity.sh")
                        .args(["cli", "getrawtransaction", txid])
                        .output()
                        .context("regtest_simplicity.sh cli getrawtransaction")?;
                    if !out.status.success() {
                        anyhow::bail!(
                            "getrawtransaction failed: {}",
                            String::from_utf8_lossy(&out.stderr)
                        );
                    }
                    Ok(String::from_utf8(out.stdout)?.trim().to_string())
                };
                let result = lab_rgb::bfa::audit_history(&hist, &fetch)?;
                println!("{}", serde_json::to_string_pretty(&result)?);
                if !result.ok {
                    anyhow::bail!("{}", result.summary);
                }
            }
            BfaCmd::Demo { script } => {
                anyhow::ensure!(
                    script.is_file(),
                    "demo script missing: {} (run from repo root)",
                    script.display()
                );
                let status = std::process::Command::new("bash")
                    .arg(&script)
                    .status()
                    .with_context(|| format!("run {}", script.display()))?;
                if !status.success() {
                    anyhow::bail!("demo exited with {status}");
                }
            }
        },
        Commands::ApiRoot => {
            println!("{}", serde_json::to_string_pretty(&lab_api::root_json())?);
        }
    }
    Ok(())
}

fn serve_labd(cfg: &Config, bind: &str) -> Result<()> {
    let listener = TcpListener::bind(bind).with_context(|| format!("bind {bind}"))?;
    eprintln!("labd listening on http://{bind}");
    eprintln!("  GET  /                 web UI (verify + swap status)");
    eprintln!("  GET  /v1/health");
    eprintln!("  GET  /v1/networks");
    eprintln!("  GET  /v1/proofs/{{id}}");
    eprintln!("  GET  /v1/swap/{{id}}     swap status (preimage redacted)");
    eprintln!("  GET  /v1/swaps          list swap ids");
    eprintln!("  GET  /demo              read-only wallet board");
    eprintln!("  GET  /v1/demo/wallets");
    eprintln!("  GET  /v1/demo/activity");
    eprintln!("  POST /v1/rgb/verify  JSON {{plan_id|plan, txid}}");

    let web_dir = PathBuf::from("web");
    let store = RgbStore::new(&cfg.data_dir);
    let swap_store = SwapStore::new(&cfg.data_dir);

    for stream in listener.incoming() {
        let mut stream = match stream {
            Ok(s) => s,
            Err(_) => continue,
        };
        let mut buf = [0u8; 65536];
        let n = match stream.read(&mut buf) {
            Ok(n) => n,
            Err(_) => continue,
        };
        let req = String::from_utf8_lossy(&buf[..n]);
        let mut lines = req.lines();
        let start = lines.next().unwrap_or("");
        let mut parts = start.split_whitespace();
        let method = parts.next().unwrap_or("GET");
        // strip query string
        let path_raw = parts.next().unwrap_or("/");
        let path = path_raw.split('?').next().unwrap_or(path_raw);

        let (status, content_type, body) = if method == "GET"
            && (path == "/" || path == "/index.html")
        {
            let html = fs::read_to_string(web_dir.join("index.html")).unwrap_or_else(|_| {
                "<html><body><h1>rgbmvp verifier</h1><p>missing web/index.html</p></body></html>"
                    .into()
            });
            ("200 OK", "text/html; charset=utf-8", html.into_bytes())
        } else if method == "GET" && (path == "/demo" || path == "/demo.html") {
            let html = fs::read_to_string(web_dir.join("demo.html")).unwrap_or_else(|_| {
                "<html><body><h1>/demo</h1><p>missing web/demo.html</p></body></html>".into()
            });
            ("200 OK", "text/html; charset=utf-8", html.into_bytes())
        } else if method == "GET" && path == "/v1/health" {
            let report = lab_chain::network_status(cfg).unwrap_or_else(|e| {
                let mut r = lab_core::HealthReport::phase0_base(cfg.network);
                r.status = "error".into();
                r.checks.push(lab_core::HealthCheck {
                    name: "status".into(),
                    ok: false,
                    detail: Some(e.to_string()),
                });
                r
            });
            let j = serde_json::to_vec_pretty(&lab_api::health_json(&report)).unwrap();
            ("200 OK", "application/json", j)
        } else if method == "GET" && path == "/v1/networks" {
            let j = serde_json::to_vec_pretty(&serde_json::json!({
                "networks": ["liquid-testnet", "bitcoin-testnet"],
                "default": "liquid-testnet",
                "mainnet": false
            }))
            .unwrap();
            ("200 OK", "application/json", j)
        } else if method == "GET" && path.starts_with("/v1/proofs/") {
            let id = path.trim_start_matches("/v1/proofs/");
            match store.load_proof(id) {
                Ok(p) => (
                    "200 OK",
                    "application/json",
                    serde_json::to_vec_pretty(&p).unwrap(),
                ),
                Err(e) => (
                    "404 Not Found",
                    "application/json",
                    serde_json::to_vec(&serde_json::json!({"error": e.to_string()})).unwrap(),
                ),
            }
        } else if method == "GET" && path == "/v1/swaps" {
            match list_swap_ids(&cfg.data_dir) {
                Ok(ids) => (
                    "200 OK",
                    "application/json",
                    serde_json::to_vec_pretty(&serde_json::json!({ "swaps": ids })).unwrap(),
                ),
                Err(e) => (
                    "500 Internal Server Error",
                    "application/json",
                    serde_json::to_vec(&serde_json::json!({"error": e.to_string()})).unwrap(),
                ),
            }
        } else if method == "GET" && path.starts_with("/v1/swap/") {
            let id = path.trim_start_matches("/v1/swap/");
            match swap_store.load(id) {
                Ok(s) => {
                    let public = public_swap_view(&s, cfg);
                    (
                        "200 OK",
                        "application/json",
                        serde_json::to_vec_pretty(&public).unwrap(),
                    )
                }
                Err(e) => (
                    "404 Not Found",
                    "application/json",
                    serde_json::to_vec(&serde_json::json!({"error": e.to_string()})).unwrap(),
                ),
            }
        } else if method == "GET" && path == "/v1/demo/wallets" {
            match demo_wallets(cfg) {
                Ok(v) => (
                    "200 OK",
                    "application/json",
                    serde_json::to_vec_pretty(&v).unwrap(),
                ),
                Err(e) => (
                    "500 Internal Server Error",
                    "application/json",
                    serde_json::to_vec(&serde_json::json!({"error": e.to_string()})).unwrap(),
                ),
            }
        } else if method == "GET" && path == "/v1/demo/activity" {
            match demo_activity(cfg) {
                Ok(v) => (
                    "200 OK",
                    "application/json",
                    serde_json::to_vec_pretty(&v).unwrap(),
                ),
                Err(e) => (
                    "500 Internal Server Error",
                    "application/json",
                    serde_json::to_vec(&serde_json::json!({"error": e.to_string()})).unwrap(),
                ),
            }
        } else if method == "POST" && path == "/v1/rgb/verify" {
            let body_start = req.find("\r\n\r\n").map(|i| i + 4).unwrap_or(req.len());
            let body_str = &req[body_start..];
            match handle_verify_post(cfg, &store, body_str) {
                Ok(v) => (
                    "200 OK",
                    "application/json",
                    serde_json::to_vec_pretty(&v).unwrap(),
                ),
                Err(e) => (
                    "400 Bad Request",
                    "application/json",
                    serde_json::to_vec(&serde_json::json!({"error": e.to_string()})).unwrap(),
                ),
            }
        } else {
            (
                "404 Not Found",
                "application/json",
                br#"{"error":"not found"}"#.to_vec(),
            )
        };

        let resp = format!(
            "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nAccess-Control-Allow-Origin: *\r\nConnection: close\r\n\r\n",
            body.len()
        );
        let _ = stream.write_all(resp.as_bytes());
        let _ = stream.write_all(&body);
    }
    Ok(())
}

/// Public swap JSON: never expose preimage.
fn public_swap_view(s: &lab_rgb::swap::SwapSession, cfg: &Config) -> serde_json::Value {
    let btc_ex = std::env::var("BTC_TESTNET_EXPLORER")
        .unwrap_or_else(|_| "https://blockstream.info/testnet".into());
    let lq_ex = cfg.explorer_base.clone();
    let tx_link = |ex: &str, tx: &Option<String>| {
        tx.as_ref()
            .map(|t| format!("{}/tx/{}", ex.trim_end_matches('/'), t))
    };
    serde_json::json!({
        "id": s.id,
        "phase": s.phase,
        "csv_delay": s.csv_delay,
        "hash_hex": s.hash_hex,
        "preimage_hex": null,
        "preimage_redacted": true,
        "alice_btc_wallet": s.alice_btc_wallet,
        "bob_lq_wallet": s.bob_lq_wallet,
        "btc_contract_id": s.btc_contract_id,
        "lq_contract_id": s.lq_contract_id,
        "htlc_btc": {
            "address": s.htlc_btc.address_btc,
            "claimer_label": s.htlc_btc.claimer_label,
            "refund_label": s.htlc_btc.refund_label,
            "csv_delay": s.htlc_btc.csv_delay,
        },
        "htlc_lq": {
            "address": s.htlc_lq.address_liquid_unconf,
            "claimer_label": s.htlc_lq.claimer_label,
            "refund_label": s.htlc_lq.refund_label,
            "csv_delay": s.htlc_lq.csv_delay,
        },
        "btc_fund_txid": s.btc_fund_txid,
        "btc_fund_sats": s.btc_fund_sats,
        "lq_fund_txid": s.lq_fund_txid,
        "lq_fund_sats": s.lq_fund_sats,
        "lq_claim_txid": s.lq_claim_txid,
        "btc_claim_txid": s.btc_claim_txid,
        "links": {
            "btc_fund": tx_link(&btc_ex, &s.btc_fund_txid),
            "lq_fund": tx_link(&lq_ex, &s.lq_fund_txid),
            "lq_claim": tx_link(&lq_ex, &s.lq_claim_txid),
            "btc_claim": tx_link(&btc_ex, &s.btc_claim_txid),
        },
        "notes": s.notes,
        "steps": [
            {"id": "created", "done": true},
            {"id": "funded_btc", "done": s.btc_fund_txid.is_some()},
            {"id": "funded_lq", "done": s.lq_fund_txid.is_some()},
            {"id": "claimed_lq", "done": s.lq_claim_txid.is_some()},
            {"id": "claimed_btc", "done": s.btc_claim_txid.is_some()},
            {"id": "done", "done": matches!(s.phase, lab_rgb::swap::SwapPhase::Done)},
        ],
    })
}

fn list_swap_ids(data_dir: &std::path::Path) -> Result<Vec<String>> {
    let dir = data_dir.join("swaps");
    if !dir.exists() {
        return Ok(vec![]);
    }
    let mut ids: Vec<String> = std::fs::read_dir(dir)?
        .filter_map(|e| e.ok())
        .filter_map(|e| {
            let n = e.file_name().to_string_lossy().into_owned();
            n.strip_suffix(".json").map(|s| s.to_string())
        })
        .collect();
    ids.sort();
    Ok(ids)
}

/// Read-only demo board: Liquid + BTC lab wallets and balances.
fn demo_wallets(cfg: &Config) -> Result<serde_json::Value> {
    let btc = lab_btc::BtcConfig::from_env();
    let mut wallets = Vec::new();

    for name in ["alice", "bob", "carol", "maker", "lab0"] {
        if !cfg.wallet_path(name).join("descriptor").exists() {
            continue;
        }
        let addr = lab_chain::wallet_address(cfg, name, Some(0)).ok();
        let bal = lab_chain::wallet_balance(cfg, name).ok();
        let role = std::fs::read_to_string(cfg.wallet_path(name).join("meta.json"))
            .ok()
            .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
            .and_then(|v| v.get("role").and_then(|r| r.as_str().map(|x| x.to_string())));
        wallets.push(serde_json::json!({
            "name": name,
            "chain": "liquid-testnet",
            "role": role,
            "address": addr.as_ref().map(|a| &a.address),
            "lbtc_sats": bal.as_ref().map(|b| b.lbtc_sats),
            "balances_sats": bal.as_ref().map(|b| &b.balances_sats),
            "explorer": addr.as_ref().map(|a| format!(
                "{}/address/{}",
                cfg.explorer_base.trim_end_matches('/'),
                a.address
            )),
            "error": if addr.is_none() { Some("load failed") } else { None::<&str> },
        }));
    }

    if lab_btc::wallet_exists(cfg, "btc-alice") {
        let info = lab_btc::load_wallet_address(cfg, &btc, "btc-alice").ok();
        let bal = lab_btc::balance(cfg, &btc, "btc-alice").ok();
        wallets.push(serde_json::json!({
            "name": "btc-alice",
            "chain": "bitcoin-testnet",
            "role": "btc-alice",
            "address": info.as_ref().map(|i| &i.address),
            "btc_sats": bal.as_ref().map(|b| b.balance_sats),
            "utxo_count": bal.as_ref().map(|b| b.utxo_count),
            "explorer": info.as_ref().map(|i| &i.explorer_url),
        }));
    }

    Ok(serde_json::json!({
        "updated": true,
        "note": "Read-only demo board. No send/swap actions from the browser.",
        "wallets": wallets,
    }))
}

/// Recent swaps, RGB transfers, proofs (ids + paths only).
fn demo_activity(cfg: &Config) -> Result<serde_json::Value> {
    let swap_ids = list_swap_ids(&cfg.data_dir)?;
    let mut swaps = Vec::new();
    let ss = SwapStore::new(&cfg.data_dir);
    for id in &swap_ids {
        if let Ok(s) = ss.load(id) {
            swaps.push(serde_json::json!({
                "id": s.id,
                "phase": s.phase,
                "btc_fund_txid": s.btc_fund_txid,
                "lq_fund_txid": s.lq_fund_txid,
                "lq_claim_txid": s.lq_claim_txid,
                "btc_claim_txid": s.btc_claim_txid,
                "status_url": format!("/v1/swap/{}", s.id),
                "ui_url": format!("/?swap={}", s.id),
            }));
        }
    }

    let mut transfers = Vec::new();
    let tdir = cfg.data_dir.join("rgb/transfers");
    if tdir.exists() {
        for e in std::fs::read_dir(&tdir)?.filter_map(|e| e.ok()) {
            let n = e.file_name().to_string_lossy().into_owned();
            if n.ends_with(".json") && !n.contains("broadcast") {
                transfers.push(n.trim_end_matches(".json").to_string());
            }
        }
        transfers.sort();
        transfers.reverse();
        transfers.truncate(20);
    }

    let mut proofs = Vec::new();
    let pdir = cfg.data_dir.join("rgb/proofs");
    if pdir.exists() {
        for e in std::fs::read_dir(&pdir)?.filter_map(|e| e.ok()) {
            let n = e.file_name().to_string_lossy().into_owned();
            if n.ends_with(".json") {
                proofs.push(n.trim_end_matches(".json").to_string());
            }
        }
        proofs.sort();
        proofs.reverse();
        proofs.truncate(20);
    }

    Ok(serde_json::json!({
        "swaps": swaps,
        "rgb_transfer_plans": transfers,
        "rgb_proofs": proofs,
    }))
}

fn handle_verify_post(
    cfg: &Config,
    store: &RgbStore,
    body: &str,
) -> Result<serde_json::Value> {
    let v: serde_json::Value = serde_json::from_str(body).context("json body")?;
    let plan_id = v
        .get("plan_id")
        .or_else(|| v.get("plan"))
        .and_then(|x| x.as_str())
        .context("plan_id required")?;
    let txid = v
        .get("txid")
        .and_then(|x| x.as_str())
        .context("txid required")?;
    let plan = store.load_transfer(plan_id)?;
    let api = lab_chain::esplora_api_base(cfg);
    let witness = lab_chain::fetch_witness_esplora(&api, txid)?;
    let result = verify_against_witness(&plan, &witness, &cfg.explorer_base)?;
    let proof_id = format!("proof-{}", &txid[..16.min(txid.len())]);
    let path = store.save_proof(&proof_id, &result)?;
    Ok(serde_json::json!({
        "proof_id": proof_id,
        "stored": path.display().to_string(),
        "result": result,
    }))
}
