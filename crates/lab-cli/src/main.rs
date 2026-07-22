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
    issue_nia, plan_claim_transfer, plan_transfer, plan_transfer_to_seal, verify_against_witness,
    IssueRequest, DEMO_INTERNAL_XONLY_HEX,
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
    /// End-to-end C2 mint-gate burn demo (empty SPK + recursion)
    DemoC2 {
        #[arg(long, default_value = "scripts/demo_c2_mint_gate_burn.sh")]
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
        /// S3: fund assigns RGB to HTLC seal; claim re-anchors + verify for done
        #[arg(long, default_value_t = false)]
        rgb_wrap: bool,
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
        /// Override session; transfer RGB onto HTLC after value fund
        #[arg(long, default_value_t = false)]
        rgb_wrap: bool,
        #[arg(long, default_value_t = 546)]
        commitment_sats: u64,
        #[arg(long, default_value_t = 42)]
        entropy: u64,
    },
    /// Fund Liquid HTLC from bob LWK wallet
    FundLq {
        #[arg(long)]
        id: String,
        #[arg(long, default_value_t = 5000)]
        amount_sats: u64,
        #[arg(long, default_value_t = false)]
        rgb_wrap: bool,
        #[arg(long, default_value_t = 546)]
        commitment_sats: u64,
        #[arg(long, default_value_t = 42)]
        entropy: u64,
    },
    /// Alice claims Liquid HTLC (reveals preimage)
    ClaimLq {
        #[arg(long)]
        id: String,
        #[arg(long, default_value_t = 300)]
        fee_sats: u64,
        #[arg(long, default_value_t = 546)]
        commitment_sats: u64,
        #[arg(long, default_value_t = 43)]
        entropy: u64,
    },
    /// Bob claims BTC HTLC using preimage
    ClaimBtc {
        #[arg(long)]
        id: String,
        #[arg(long, default_value_t = 500)]
        fee_sats: u64,
        #[arg(long, default_value_t = 546)]
        commitment_sats: u64,
        #[arg(long, default_value_t = 44)]
        entropy: u64,
        /// Prefer preimage from Liquid claim witness (S3) instead of session file
        #[arg(long, default_value_t = false)]
        from_witness: bool,
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
    /// Extract claim preimage from a confirmed HTLC claim tx (S3 / R2)
    ExtractPreimage {
        #[arg(long)]
        chain: String,
        #[arg(long)]
        txid: String,
        /// Optional swap id: store note that preimage matched session hash
        #[arg(long)]
        id: Option<String>,
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
    /// Send sats from a named BTC wallet to an address (testnet only)
    Send {
        #[arg(long, default_value = "btc-alice")]
        from: String,
        #[arg(long)]
        to: String,
        #[arg(long)]
        amount_sats: u64,
        #[arg(long, default_value_t = 500)]
        fee_sats: u64,
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
                    rgb_wrap,
                } => {
                    let session = swap::init_swap(
                        &id,
                        csv_delay,
                        &alice_btc,
                        &bob_lq,
                        btc_contract,
                        lq_contract,
                        rgb_wrap,
                    )?;
                    let path = store.save(&session)?;
                    let mut next = vec![
                        "rgbmvp swap fund-btc --id …".to_string(),
                        "rgbmvp swap fund-lq --id …".to_string(),
                        "rgbmvp swap claim-lq --id …  # Alice reveals preimage".to_string(),
                        "rgbmvp swap claim-btc --id … # Bob claims BTC".to_string(),
                    ];
                    if rgb_wrap {
                        next = vec![
                            "rgbmvp swap fund-btc --id … --rgb-wrap  # value + RGB→HTLC seal".into(),
                            "rgbmvp swap fund-lq --id … --rgb-wrap".into(),
                            "rgbmvp swap claim-lq --id …  # preimage + re-anchor + verify".into(),
                            "rgbmvp swap claim-btc --id … --from-witness".into(),
                            "rgbmvp swap extract-preimage --chain liquid --txid <lq_claim>".into(),
                        ];
                    }
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&serde_json::json!({
                            "status": "created",
                            "stored": path.display().to_string(),
                            "rgb_wrap": session.rgb_wrap,
                            "version": session.version,
                            "session": session,
                            "next": next,
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
                    rgb_wrap,
                    commitment_sats,
                    entropy,
                } => {
                    let mut s = store.load(&id)?;
                    let do_wrap = rgb_wrap || s.rgb_wrap;
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
                    let mut rgb_meta = serde_json::Value::Null;
                    if do_wrap {
                        let rgb_store = RgbStore::new(&cfg.data_dir);
                        rgb_meta = s3_fund_wrap_btc(
                            &cfg,
                            &btc,
                            &rgb_store,
                            &mut s,
                            commitment_sats,
                            entropy,
                        )?;
                    }
                    swap::recompute_phase(&mut s);
                    store.save(&s)?;
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&serde_json::json!({
                            "status": "funded_btc",
                            "phase": s.phase,
                            "rgb_wrap": do_wrap,
                            "broadcast": bc,
                            "htlc_address": s.htlc_btc.address_btc,
                            "rgb": rgb_meta,
                        }))?
                    );
                }
                SwapCmd::FundLq {
                    id,
                    amount_sats,
                    rgb_wrap,
                    commitment_sats,
                    entropy,
                } => {
                    let mut s = store.load(&id)?;
                    let do_wrap = rgb_wrap || s.rgb_wrap;
                    // Prefer an existing HTLC UTXO (retry after wrap failure, or manual fund).
                    // Critical for S3: value fund must not re-spend the RGB issue seal.
                    let existing = lab_chain::find_address_utxo(
                        &cfg,
                        &s.htlc_lq.address_liquid_unconf,
                        amount_sats.saturating_sub(1),
                    )
                    .ok();
                    let (bc_val, ftxid, fvout, fval, reused) = if let Some((tx, vo, va)) = existing {
                        (
                            serde_json::json!({
                                "txid": tx,
                                "note": "reused existing HTLC UTXO (no new send_lbtc)",
                                "reused": true,
                            }),
                            tx,
                            vo,
                            va,
                            true,
                        )
                    } else {
                        // When rgb_wrap, refuse if issue seal is the only large UTXO
                        // (would be spent by send and break wrap). Best-effort check.
                        if do_wrap {
                            if let Some(cid) = s.lq_contract_id.as_ref() {
                                let rgb_store = RgbStore::new(&cfg.data_dir);
                                if let Ok(issue) = rgb_store.load_issue(cid) {
                                    let utxos = lab_chain::wallet_utxos(&cfg, &s.bob_lq_wallet)?;
                                    let large: Vec<_> = utxos
                                        .iter()
                                        .filter(|u| u.value >= amount_sats.saturating_add(500))
                                        .collect();
                                    if large.len() == 1 && large[0].outpoint == issue.seal {
                                        anyhow::bail!(
                                            "S3 fund-lq: only spendable UTXO is the RGB issue seal {}. \
                                             Split funds first (wallet send to self) or re-issue on a \
                                             UTXO that will not be used for HTLC value.",
                                            issue.seal
                                        );
                                    }
                                }
                            }
                        }
                        let bc = lab_chain::send_lbtc(
                            &cfg,
                            &s.bob_lq_wallet,
                            &s.htlc_lq.address_liquid_unconf,
                            amount_sats,
                        )?;
                        let (tx, vo, va) = lab_chain::find_address_utxo(
                            &cfg,
                            &s.htlc_lq.address_liquid_unconf,
                            amount_sats.saturating_sub(1),
                        )
                        .unwrap_or((bc.txid.clone(), 0, amount_sats));
                        (serde_json::to_value(&bc)?, tx, vo, va, false)
                    };
                    s.lq_fund_txid = Some(ftxid);
                    s.lq_fund_vout = Some(fvout);
                    s.lq_fund_sats = Some(fval);
                    // Persist value fund even if RGB wrap fails later.
                    swap::recompute_phase(&mut s);
                    store.save(&s)?;
                    let mut rgb_meta = serde_json::Value::Null;
                    if do_wrap {
                        let rgb_store = RgbStore::new(&cfg.data_dir);
                        match s3_fund_wrap_lq(
                            &cfg,
                            &rgb_store,
                            &mut s,
                            commitment_sats,
                            entropy,
                        ) {
                            Ok(m) => {
                                rgb_meta = m;
                                swap::recompute_phase(&mut s);
                                store.save(&s)?;
                            }
                            Err(e) => {
                                store.save(&s)?;
                                anyhow::bail!(
                                    "LQ value funded (txid {}) but RGB wrap failed: {e}. \
                                     Fix seal (re-issue on unspent UTXO) then re-run fund-lq --rgb-wrap \
                                     (HTLC UTXO will be reused).",
                                    s.lq_fund_txid.as_deref().unwrap_or("?")
                                );
                            }
                        }
                    }
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&serde_json::json!({
                            "status": "funded_lq",
                            "phase": s.phase,
                            "rgb_wrap": do_wrap,
                            "reused_htlc_utxo": reused,
                            "broadcast": bc_val,
                            "htlc_address": s.htlc_lq.address_liquid_unconf,
                            "htlc_seal": format!("{}:{}", s.lq_fund_txid.as_deref().unwrap_or(""), s.lq_fund_vout.unwrap_or(0)),
                            "rgb": rgb_meta,
                        }))?
                    );
                }
                SwapCmd::ClaimLq {
                    id,
                    fee_sats,
                    commitment_sats,
                    entropy,
                } => {
                    let mut s = store.load(&id)?;
                    let rgb_store = RgbStore::new(&cfg.data_dir);
                    let mut out = if s.rgb_wrap && s.lq_contract_id.is_some() {
                        s3_claim_lq_rgb(&cfg, &rgb_store, &mut s, fee_sats, commitment_sats, entropy)?
                    } else {
                        s3_claim_lq_value(&cfg, &mut s, fee_sats)?
                    };
                    swap::recompute_phase(&mut s);
                    store.save(&s)?;
                    out["phase"] = serde_json::json!(s.phase);
                    println!("{}", serde_json::to_string_pretty(&out)?);
                }
                SwapCmd::ClaimBtc {
                    id,
                    fee_sats,
                    commitment_sats,
                    entropy,
                    from_witness,
                } => {
                    let mut s = store.load(&id)?;
                    let rgb_store = RgbStore::new(&cfg.data_dir);
                    let preimage = if from_witness {
                        resolve_preimage_from_lq_claim(&cfg, &s)?
                    } else {
                        hex::decode(&s.preimage_hex)?
                    };
                    let mut out = if s.rgb_wrap && s.btc_contract_id.is_some() {
                        s3_claim_btc_rgb(
                            &cfg,
                            &rgb_store,
                            &mut s,
                            &preimage,
                            fee_sats,
                            commitment_sats,
                            entropy,
                        )?
                    } else {
                        s3_claim_btc_value(&cfg, &mut s, &preimage, fee_sats)?
                    };
                    swap::recompute_phase(&mut s);
                    store.save(&s)?;
                    out["phase"] = serde_json::json!(s.phase);
                    println!("{}", serde_json::to_string_pretty(&out)?);
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
                SwapCmd::ExtractPreimage { chain, txid, id } => {
                    let pre = extract_preimage_cli(&cfg, &chain, &txid)?;
                    let pre_hex = hex::encode(pre);
                    let hash = htlc::sha256_preimage(&pre);
                    let mut matched = None;
                    if let Some(sid) = id.as_ref() {
                        let mut s = store.load(sid)?;
                        let session_hash = hex::decode(&s.hash_hex)?;
                        let ok = session_hash.as_slice() == hash.as_slice();
                        matched = Some(ok);
                        if ok {
                            s.notes.push(format!(
                                "extract-preimage matched hash from {chain} tx {txid}"
                            ));
                            store.save(&s)?;
                        }
                    }
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&serde_json::json!({
                            "status": "ok",
                            "chain": chain,
                            "txid": txid,
                            "preimage_hex": pre_hex,
                            "hash_hex": hex::encode(hash),
                            "session_hash_match": matched,
                            "note": "Preimage is public once claim is mined; still never log in labd GET.",
                        }))?
                    );
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
                BtcCmd::Send {
                    from,
                    to,
                    amount_sats,
                    fee_sats,
                } => {
                    let bc = lab_btc::fund_address(&cfg, &btc, &from, &to, amount_sats, fee_sats)?;
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&serde_json::json!({
                            "status": "sent",
                            "from": from,
                            "to": to,
                            "amount_sats": amount_sats,
                            "fee_sats": fee_sats,
                            "broadcast": bc,
                        }))?
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
            CovenantCmd::DemoC2 { script } => {
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
    eprintln!("  GET  /                 lab console (Issue · Transfer · Verify · Swap)");
    eprintln!("  GET  /demo             read-only board");
    eprintln!("  GET  /audit            BFA audit UI");
    eprintln!("  GET  /v1               API catalog");
    eprintln!("  GET  /v1/health · /v1/phases · /v1/networks");
    eprintln!("  GET  /v1/proofs/{{id}} · /v1/swaps · /v1/swap/{{id}}");
    eprintln!("  GET  /v1/demo/wallets · /v1/demo/activity");
    eprintln!("  GET  /v1/rgb/contracts · /v1/rgb/plans/{{id}}");
    eprintln!("  POST /v1/rgb/issue · transfer · verify");
    eprintln!("  POST /v1/swap/init");
    eprintln!("  POST /v1/swap/{{id}}/action  fund_btc|fund_lq|claim_lq|claim_btc|refund_*");
    eprintln!("  POST /v1/audit/bfa");

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

        // CORS preflight for browser tools
        let (status, content_type, body) = if method == "OPTIONS" {
            (
                "204 No Content",
                "text/plain",
                Vec::new(),
            )
        } else if method == "GET"
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
        } else if method == "GET" && (path == "/audit" || path == "/audit.html") {
            let html = fs::read_to_string(web_dir.join("audit.html")).unwrap_or_else(|_| {
                "<html><body><h1>/audit</h1><p>missing web/audit.html</p></body></html>".into()
            });
            ("200 OK", "text/html; charset=utf-8", html.into_bytes())
        } else if method == "GET" && path == "/v1" {
            let j = serde_json::to_vec_pretty(&lab_api::root_json()).unwrap();
            ("200 OK", "application/json", j)
        } else if method == "GET" && path == "/v1/phases" {
            let j = serde_json::to_vec_pretty(&lab_api::phases_json()).unwrap();
            ("200 OK", "application/json", j)
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
        } else if method == "GET" && path.starts_with("/v1/swap/") && !path.contains("/action") {
            let id = path.trim_start_matches("/v1/swap/");
            // strip trailing slash
            let id = id.trim_end_matches('/');
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
                    serde_json::to_vec(&serde_json::json!({"error": e.to_string(), "status": "error"})).unwrap(),
                ),
            }
        } else if method == "POST" && path == "/v1/swap/init" {
            let body_start = req.find("\r\n\r\n").map(|i| i + 4).unwrap_or(req.len());
            let body_str = &req[body_start..];
            match handle_swap_init_post(cfg, &swap_store, body_str) {
                Ok(v) => (
                    "200 OK",
                    "application/json",
                    serde_json::to_vec_pretty(&v).unwrap(),
                ),
                Err(e) => (
                    "400 Bad Request",
                    "application/json",
                    serde_json::to_vec(&serde_json::json!({"error": e.to_string(), "status": "error"})).unwrap(),
                ),
            }
        } else if method == "POST" && path.starts_with("/v1/swap/") && path.ends_with("/action") {
            // /v1/swap/{id}/action
            let mid = path
                .trim_start_matches("/v1/swap/")
                .trim_end_matches("/action")
                .trim_end_matches('/');
            let body_start = req.find("\r\n\r\n").map(|i| i + 4).unwrap_or(req.len());
            let body_str = &req[body_start..];
            match handle_swap_action_post(cfg, &swap_store, mid, body_str) {
                Ok(v) => (
                    "200 OK",
                    "application/json",
                    serde_json::to_vec_pretty(&v).unwrap(),
                ),
                Err(e) => (
                    "400 Bad Request",
                    "application/json",
                    serde_json::to_vec(&serde_json::json!({"error": e.to_string(), "status": "error"})).unwrap(),
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
        } else if method == "GET" && path == "/v1/rgb/contracts" {
            match list_rgb_contracts(cfg) {
                Ok(v) => (
                    "200 OK",
                    "application/json",
                    serde_json::to_vec_pretty(&v).unwrap(),
                ),
                Err(e) => (
                    "500 Internal Server Error",
                    "application/json",
                    serde_json::to_vec(&serde_json::json!({"error": e.to_string(), "status": "error"})).unwrap(),
                ),
            }
        } else if method == "GET" && path.starts_with("/v1/rgb/plans/") {
            let id = path.trim_start_matches("/v1/rgb/plans/");
            match store.load_transfer(id) {
                Ok(p) => (
                    "200 OK",
                    "application/json",
                    serde_json::to_vec_pretty(&serde_json::json!({"plan_id": id, "plan": p})).unwrap(),
                ),
                Err(e) => (
                    "404 Not Found",
                    "application/json",
                    serde_json::to_vec(&serde_json::json!({"error": e.to_string(), "status": "error"})).unwrap(),
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
                    serde_json::to_vec(&serde_json::json!({"error": e.to_string(), "status": "error"})).unwrap(),
                ),
            }
        } else if method == "POST" && path == "/v1/rgb/issue" {
            let body_start = req.find("\r\n\r\n").map(|i| i + 4).unwrap_or(req.len());
            let body_str = &req[body_start..];
            match handle_rgb_issue_post(cfg, &store, body_str) {
                Ok(v) => (
                    "200 OK",
                    "application/json",
                    serde_json::to_vec_pretty(&v).unwrap(),
                ),
                Err(e) => (
                    "400 Bad Request",
                    "application/json",
                    serde_json::to_vec(&serde_json::json!({"error": e.to_string(), "status": "error"})).unwrap(),
                ),
            }
        } else if method == "POST" && path == "/v1/rgb/transfer" {
            let body_start = req.find("\r\n\r\n").map(|i| i + 4).unwrap_or(req.len());
            let body_str = &req[body_start..];
            match handle_rgb_transfer_post(cfg, &store, body_str) {
                Ok(v) => (
                    "200 OK",
                    "application/json",
                    serde_json::to_vec_pretty(&v).unwrap(),
                ),
                Err(e) => (
                    "400 Bad Request",
                    "application/json",
                    serde_json::to_vec(&serde_json::json!({"error": e.to_string(), "status": "error"})).unwrap(),
                ),
            }
        } else if method == "POST" && path == "/v1/audit/bfa" {
            let body_start = req.find("\r\n\r\n").map(|i| i + 4).unwrap_or(req.len());
            let body_str = &req[body_start..];
            match handle_bfa_audit_post(body_str) {
                Ok(v) => {
                    let code = if v.ok {
                        "200 OK"
                    } else {
                        "422 Unprocessable Entity"
                    };
                    (
                        code,
                        "application/json",
                        serde_json::to_vec_pretty(&v).unwrap(),
                    )
                }
                Err(e) => (
                    "400 Bad Request",
                    "application/json",
                    serde_json::to_vec(&serde_json::json!({"error": e.to_string(), "status": "error"})).unwrap(),
                ),
            }
        } else {
            (
                "404 Not Found",
                "application/json",
                br#"{"error":"not found","status":"error"}"#.to_vec(),
            )
        };

        let resp = format!(
            "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nAccess-Control-Allow-Origin: *\r\nAccess-Control-Allow-Methods: GET, POST, OPTIONS\r\nAccess-Control-Allow-Headers: Content-Type\r\nConnection: close\r\n\r\n",
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
    let leg_public = |leg: &Option<lab_rgb::swap::SwapLegRgb>| {
        leg.as_ref().map(|r| {
            serde_json::json!({
                "contract_id": r.contract_id,
                "amount": r.amount,
                "htlc_seal": r.htlc_seal,
                "fund_plan_id": r.fund_plan_id,
                "fund_anchor_txid": r.fund_anchor_txid,
                "fund_verify": r.fund_verify,
                "claim_plan_id": r.claim_plan_id,
                "claim_anchor_txid": r.claim_anchor_txid,
                "claim_verify": r.claim_verify,
                "successor_seal": r.successor_seal,
            })
        })
    };
    serde_json::json!({
        "id": s.id,
        "version": s.version,
        "phase": s.phase,
        "csv_delay": s.csv_delay,
        "hash_hex": s.hash_hex,
        "preimage_hex": null,
        "preimage_redacted": true,
        "rgb_wrap": s.rgb_wrap,
        "alice_btc_wallet": s.alice_btc_wallet,
        "bob_lq_wallet": s.bob_lq_wallet,
        "btc_contract_id": s.btc_contract_id,
        "lq_contract_id": s.lq_contract_id,
        "btc_rgb": leg_public(&s.btc_rgb),
        "lq_rgb": leg_public(&s.lq_rgb),
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
            {"id": "created", "done": true, "label": "Created"},
            {"id": "funded_btc", "done": s.btc_fund_txid.is_some(), "label": "Fund BTC HTLC"},
            {"id": "funded_lq", "done": s.lq_fund_txid.is_some(), "label": "Fund Liquid HTLC"},
            {"id": "claimed_lq", "done": s.lq_claim_txid.is_some(), "label": "Alice claims LQ (reveals preimage)"},
            {"id": "claimed_btc", "done": s.btc_claim_txid.is_some(), "label": "Bob claims BTC"},
            {"id": "done", "done": matches!(s.phase, lab_rgb::swap::SwapPhase::Done), "label": "Done"},
        ],
        "next_actions": swap_next_actions(s),
        "guide": swap_guide(s),
    })
}

/// Which mutations the lab console should offer (server-side keys).
fn swap_next_actions(s: &lab_rgb::swap::SwapSession) -> Vec<serde_json::Value> {
    use lab_rgb::swap::SwapPhase;
    let mut out = Vec::new();
    if matches!(s.phase, SwapPhase::Refunded | SwapPhase::Done) {
        return out;
    }
    if s.btc_fund_txid.is_none() {
        out.push(serde_json::json!({
            "action": "fund_btc",
            "label": "1. Fund BTC HTLC",
            "defaults": {"amount_sats": 10000, "fee_sats": 800},
            "role": "alice (btc-alice)"
        }));
    }
    if s.lq_fund_txid.is_none() {
        out.push(serde_json::json!({
            "action": "fund_lq",
            "label": "2. Fund Liquid HTLC",
            "defaults": {"amount_sats": 5000},
            "role": "bob"
        }));
    }
    if s.btc_fund_txid.is_some() && s.lq_fund_txid.is_some() && s.lq_claim_txid.is_none() {
        out.push(serde_json::json!({
            "action": "claim_lq",
            "label": "3. Claim Liquid (Alice reveals preimage)",
            "defaults": {"fee_sats": 300},
            "role": "alice"
        }));
    }
    if s.lq_claim_txid.is_some() && s.btc_claim_txid.is_none() {
        out.push(serde_json::json!({
            "action": "claim_btc",
            "label": "4. Claim BTC (Bob uses preimage)",
            "defaults": {"fee_sats": 500},
            "role": "bob"
        }));
    }
    // Refunds only offered if funded and not claimed on that leg
    if s.btc_fund_txid.is_some() && s.btc_claim_txid.is_none() {
        out.push(serde_json::json!({
            "action": "refund_btc",
            "label": "Refund BTC (after CSV)",
            "defaults": {"fee_sats": 500},
            "role": "alice",
            "caution": "Requires csv_delay confirmations since fund"
        }));
    }
    if s.lq_fund_txid.is_some() && s.lq_claim_txid.is_none() {
        out.push(serde_json::json!({
            "action": "refund_lq",
            "label": "Refund Liquid (after CSV)",
            "defaults": {"fee_sats": 300},
            "role": "bob",
            "caution": "Requires csv_delay confirmations since fund"
        }));
    }
    out
}

fn swap_guide(s: &lab_rgb::swap::SwapSession) -> String {
    if matches!(s.phase, lab_rgb::swap::SwapPhase::Done) {
        return "Swap complete. Preimage was revealed on Liquid claim; never shown in this UI.".into();
    }
    if matches!(s.phase, lab_rgb::swap::SwapPhase::Refunded) {
        return "Refund path used. Happy-path claim is no longer available.".into();
    }
    if s.btc_fund_txid.is_none() && s.lq_fund_txid.is_none() {
        return "Create or load a swap, then fund BTC (Alice) and Liquid (Bob) HTLCs.".into();
    }
    if s.btc_fund_txid.is_none() {
        return "Fund the Bitcoin HTLC from btc-alice next.".into();
    }
    if s.lq_fund_txid.is_none() {
        return "Fund the Liquid HTLC from bob next.".into();
    }
    if s.lq_claim_txid.is_none() {
        return "Both legs funded. Alice should claim Liquid (this publishes the preimage on-chain).".into();
    }
    if s.btc_claim_txid.is_none() {
        return "Liquid claimed. Bob can claim BTC with the published preimage.".into();
    }
    "Almost done — refresh status.".into()
}

/// Map a pasted address to the lab wallet *name* when possible.
fn resolve_btc_wallet_name(s: &str) -> String {
    let t = s.trim();
    if t == "btc-alice"
        || t.eq_ignore_ascii_case("alice-btc")
        || t == "tb1q85aadpqgzjgrgp69gf2ejf0883yx7s9wy85h4p"
    {
        return "btc-alice".into();
    }
    // If user pasted a bech32 address, they almost always meant btc-alice in this lab.
    if t.starts_with("tb1") || t.starts_with("bc1") {
        return "btc-alice".into();
    }
    t.to_string()
}

fn resolve_lq_wallet_name(s: &str) -> String {
    let t = s.trim();
    if t.starts_with("tlq1") || t.starts_with("el1") || t.starts_with("lq1") {
        // Liquid addresses are not wallet names — default counterparty is bob.
        return "bob".into();
    }
    t.to_string()
}

fn handle_swap_init_post(
    cfg: &Config,
    store: &SwapStore,
    body: &str,
) -> Result<serde_json::Value> {
    let v: serde_json::Value = serde_json::from_str(body).context("json body")?;
    let id = v
        .get("id")
        .and_then(|x| x.as_str())
        .context("id required")?
        .to_string();
    let csv_delay = v.get("csv_delay").and_then(|x| x.as_u64()).unwrap_or(6) as u32;
    let alice_btc = resolve_btc_wallet_name(
        v.get("alice_btc")
            .and_then(|x| x.as_str())
            .unwrap_or("btc-alice"),
    );
    let bob_lq = resolve_lq_wallet_name(
        v.get("bob_lq")
            .and_then(|x| x.as_str())
            .unwrap_or("bob"),
    );
    let btc_contract = v
        .get("btc_contract")
        .or_else(|| v.get("btc_contract_id"))
        .and_then(|x| x.as_str())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());
    let lq_contract = v
        .get("lq_contract")
        .or_else(|| v.get("lq_contract_id"))
        .and_then(|x| x.as_str())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());
    // refuse overwrite of existing without force
    if store.path_exists(&id)
        && !v.get("force").and_then(|x| x.as_bool()).unwrap_or(false)
    {
        anyhow::bail!("swap {id} already exists; pass force:true to overwrite");
    }
    let rgb_wrap = v.get("rgb_wrap").and_then(|x| x.as_bool()).unwrap_or(false);
    let session = swap::init_swap(
        &id,
        csv_delay,
        &alice_btc,
        &bob_lq,
        btc_contract,
        lq_contract,
        rgb_wrap,
    )?;
    let path = store.save(&session)?;
    let _ = cfg;
    Ok(serde_json::json!({
        "status": "created",
        "stored": path.display().to_string(),
        "rgb_wrap": session.rgb_wrap,
        "swap": public_swap_view(&session, cfg),
        "note": "Preimage stored only under .rgbmvp/swaps/ (mode 600). Never returned by GET /v1/swap/*.",
    }))
}

fn handle_swap_action_post(
    cfg: &Config,
    store: &SwapStore,
    id: &str,
    body: &str,
) -> Result<serde_json::Value> {
    let v: serde_json::Value = serde_json::from_str(body).unwrap_or(serde_json::json!({}));
    let action = v
        .get("action")
        .and_then(|x| x.as_str())
        .context("action required (fund_btc|fund_lq|claim_lq|claim_btc|refund_btc|refund_lq)")?;
    let mut s = store.load(id)?;

    // Repair mistaken address-as-name from older sessions
    if s.alice_btc_wallet.starts_with("tb1") || s.alice_btc_wallet.starts_with("bc1") {
        s.alice_btc_wallet = resolve_btc_wallet_name(&s.alice_btc_wallet);
        store.save(&s)?;
    }
    if s.bob_lq_wallet.starts_with("tlq1")
        || s.bob_lq_wallet.starts_with("el1")
        || s.bob_lq_wallet.starts_with("lq1")
    {
        s.bob_lq_wallet = resolve_lq_wallet_name(&s.bob_lq_wallet);
        store.save(&s)?;
    }

    let result = match action {
        "set_contracts" => {
            if let Some(c) = v
                .get("btc_contract")
                .or_else(|| v.get("btc_contract_id"))
                .and_then(|x| x.as_str())
                .filter(|s| !s.is_empty())
            {
                s.btc_contract_id = Some(c.to_string());
            }
            if let Some(c) = v
                .get("lq_contract")
                .or_else(|| v.get("lq_contract_id"))
                .and_then(|x| x.as_str())
                .filter(|s| !s.is_empty())
            {
                s.lq_contract_id = Some(c.to_string());
            }
            store.save(&s)?;
            serde_json::json!({
                "status": "contracts_updated",
                "btc_contract_id": s.btc_contract_id,
                "lq_contract_id": s.lq_contract_id,
                "note": "Twin RGB ids stored on session for documentation; HTLC path unchanged.",
            })
        }
        "fund_btc" => {
            let amount_sats = v
                .get("amount_sats")
                .and_then(|x| x.as_u64())
                .unwrap_or(10_000);
            let fee_sats = v.get("fee_sats").and_then(|x| x.as_u64()).unwrap_or(800);
            let btc = lab_btc::BtcConfig::from_env();
            let wallet = resolve_btc_wallet_name(&s.alice_btc_wallet);
            s.alice_btc_wallet = wallet.clone();
            let bc = lab_btc::fund_address(
                cfg,
                &btc,
                &wallet,
                &s.htlc_btc.address_btc,
                amount_sats,
                fee_sats,
            )?;
            s.btc_fund_txid = Some(bc.txid.clone());
            s.btc_fund_vout = Some(0);
            s.btc_fund_sats = Some(amount_sats);
            swap::recompute_phase(&mut s);
            store.save(&s)?;
            serde_json::json!({
                "status": "funded_btc",
                "broadcast": bc,
                "htlc_address": s.htlc_btc.address_btc,
            })
        }
        "fund_lq" => {
            let amount_sats = v
                .get("amount_sats")
                .and_then(|x| x.as_u64())
                .unwrap_or(5_000);
            let bc = lab_chain::send_lbtc(
                cfg,
                &s.bob_lq_wallet,
                &s.htlc_lq.address_liquid_unconf,
                amount_sats,
            )?;
            s.lq_fund_txid = Some(bc.txid.clone());
            s.lq_fund_vout = Some(0);
            s.lq_fund_sats = Some(amount_sats);
            swap::recompute_phase(&mut s);
            store.save(&s)?;
            serde_json::json!({
                "status": "funded_lq",
                "broadcast": bc,
                "htlc_address": s.htlc_lq.address_liquid_unconf,
            })
        }
        "claim_lq" => {
            let fee_sats = v.get("fee_sats").and_then(|x| x.as_u64()).unwrap_or(300);
            let amount = s.lq_fund_sats.context("lq not funded (run fund_lq)")?;
            let (txid, vout, value) = lab_chain::find_address_utxo(
                cfg,
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
            let claim_txid = lab_chain::broadcast_raw_hex(cfg, &raw)?;
            s.lq_claim_txid = Some(claim_txid.clone());
            swap::recompute_phase(&mut s);
            store.save(&s)?;
            serde_json::json!({
                "status": "claimed_lq",
                "txid": claim_txid,
                "explorer": format!("{}/tx/{}", cfg.explorer_base, claim_txid),
                "preimage_published": true,
                "note": "Preimage is public on Liquid; Bob can claim BTC. Not returned in API JSON.",
            })
        }
        "claim_btc" => {
            let fee_sats = v.get("fee_sats").and_then(|x| x.as_u64()).unwrap_or(500);
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
            serde_json::json!({
                "status": "claimed_btc",
                "txid": txid,
                "explorer": format!("{}/tx/{}", btc.explorer_base, txid),
                "dest": dest.to_string(),
            })
        }
        "refund_btc" => {
            let fee_sats = v.get("fee_sats").and_then(|x| x.as_u64()).unwrap_or(500);
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
            serde_json::json!({
                "status": "refunded_btc",
                "txid": txid,
                "explorer": format!("{}/tx/{}", btc.explorer_base, txid),
                "note": "Requires CSV maturity (nSequence = csv_delay blocks) since fund.",
            })
        }
        "refund_lq" => {
            let fee_sats = v.get("fee_sats").and_then(|x| x.as_u64()).unwrap_or(300);
            if s.lq_claim_txid.is_some() {
                anyhow::bail!("Liquid already claimed; cannot refund");
            }
            let amount = s.lq_fund_sats.context("lq not funded")?;
            let (txid, vout, value) = lab_chain::find_address_utxo(
                cfg,
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
            let claim_txid = lab_chain::broadcast_raw_hex(cfg, &raw)?;
            s.notes.push(format!("lq_refund_txid={claim_txid}"));
            s.phase = lab_rgb::swap::SwapPhase::Refunded;
            store.save(&s)?;
            serde_json::json!({
                "status": "refunded_lq",
                "txid": claim_txid,
                "explorer": format!("{}/tx/{}", cfg.explorer_base, claim_txid),
                "note": "Requires CSV maturity since fund.",
            })
        }
        other => anyhow::bail!("unknown action {other:?}"),
    };

    // reload for public view
    let s2 = store.load(id)?;
    Ok(serde_json::json!({
        "action": action,
        "result": result,
        "swap": public_swap_view(&s2, cfg),
    }))
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

fn list_rgb_contracts(cfg: &Config) -> Result<serde_json::Value> {
    let dir = cfg.data_dir.join("rgb/contracts");
    let mut contracts = Vec::new();
    if dir.exists() {
        for e in fs::read_dir(&dir)?.filter_map(|e| e.ok()) {
            let p = e.path();
            if p.extension().and_then(|x| x.to_str()) != Some("json") {
                continue;
            }
            if let Ok(raw) = fs::read_to_string(&p) {
                if let Ok(v) = serde_json::from_str::<lab_rgb::IssueResult>(&raw) {
                    contracts.push(v);
                }
            }
        }
    }
    contracts.sort_by(|a, b| a.contract_id.cmp(&b.contract_id));
    Ok(serde_json::json!({ "contracts": contracts, "count": contracts.len() }))
}

/// POST /v1/rgb/issue — server-side keys (lab fixtures). JSON:
/// `{ "wallet":"alice", "name":"…", "ticker":"tRGB", "supply":1000000, "chain":"liquid-testnet", "seal":null }`
fn handle_rgb_issue_post(
    cfg: &Config,
    store: &RgbStore,
    body: &str,
) -> Result<serde_json::Value> {
    let v: serde_json::Value = serde_json::from_str(body).context("json body")?;
    let wallet = v
        .get("wallet")
        .and_then(|x| x.as_str())
        .unwrap_or("alice");
    let name = v
        .get("name")
        .and_then(|x| x.as_str())
        .unwrap_or("Test RGB")
        .to_string();
    let ticker = v
        .get("ticker")
        .and_then(|x| x.as_str())
        .unwrap_or("tRGB")
        .to_string();
    let supply = v
        .get("supply")
        .and_then(|x| x.as_u64())
        .unwrap_or(1_000_000);
    let chain = v
        .get("chain")
        .and_then(|x| x.as_str())
        .unwrap_or("liquid-testnet")
        .to_string();
    let seal = if let Some(s) = v.get("seal").and_then(|x| x.as_str()) {
        s.to_string()
    } else if chain.starts_with("bitcoin") || chain == "testnet" || chain == "testnet3" {
        let btc = lab_btc::BtcConfig::from_env();
        lab_btc::pick_largest_utxo(cfg, &btc, wallet)?.outpoint
    } else {
        lab_chain::pick_lbtc_seal(cfg, wallet)?.outpoint
    };
    let issue = issue_nia(&IssueRequest {
        name,
        ticker,
        supply,
        seal: seal.clone(),
        chain: chain.clone(),
    })?;
    let path = store.save_issue(&issue)?;
    Ok(serde_json::json!({
        "status": "issued",
        "issue": issue,
        "stored": path.display().to_string(),
        "note": "Genesis is off-chain; seal UTXO must be closed by a transfer witness tx. Keys never left labd.",
    }))
}

/// POST /v1/rgb/transfer — plan (+ optional broadcast). JSON:
/// `{ "contract":"rgb:…"|ticker path, "wallet":"alice", "amount":600000, "broadcast":false, … }`
fn handle_rgb_transfer_post(
    cfg: &Config,
    store: &RgbStore,
    body: &str,
) -> Result<serde_json::Value> {
    let v: serde_json::Value = serde_json::from_str(body).context("json body")?;
    let contract = v
        .get("contract")
        .or_else(|| v.get("contract_id"))
        .and_then(|x| x.as_str())
        .context("contract required")?;
    let wallet = v
        .get("wallet")
        .and_then(|x| x.as_str())
        .unwrap_or("alice");
    let amount = v
        .get("amount")
        .and_then(|x| x.as_u64())
        .unwrap_or(600_000);
    let broadcast = v
        .get("broadcast")
        .and_then(|x| x.as_bool())
        .unwrap_or(false);
    let entropy = v.get("entropy").and_then(|x| x.as_u64()).unwrap_or(42);
    let bob_sats = v.get("bob_sats").and_then(|x| x.as_u64()).unwrap_or(1000);
    let commitment_sats = v
        .get("commitment_sats")
        .and_then(|x| x.as_u64())
        .unwrap_or(500);
    let bob_address = v
        .get("bob_address")
        .and_then(|x| x.as_str())
        .map(|s| s.to_string());

    let issue = store
        .load_issue(contract)
        .or_else(|_| {
            // try load by scanning contracts for matching contract_id
            let data = &cfg.data_dir;
            let dir = data.join("rgb/contracts");
            if dir.exists() {
                for e in fs::read_dir(&dir)?.filter_map(|e| e.ok()) {
                    let p = e.path();
                    if p.extension().and_then(|x| x.to_str()) != Some("json") {
                        continue;
                    }
                    if let Ok(raw) = fs::read_to_string(&p) {
                        if let Ok(iss) = serde_json::from_str::<lab_rgb::IssueResult>(&raw) {
                            if iss.contract_id == contract || p.file_stem().map(|s| s.to_string_lossy()) == Some(contract.into()) {
                                return Ok(iss);
                            }
                        }
                    }
                }
            }
            anyhow::bail!("contract not found: {contract}");
        })?;

    let chain = v
        .get("chain")
        .and_then(|x| x.as_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| {
            if issue.chain_net.starts_with("bitcoin") {
                issue.chain_net.clone()
            } else {
                "liquid-testnet".into()
            }
        });

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
        "verify_hint": {
            "plan_id": plan_id,
            "next": "After broadcast, POST /v1/rgb/verify with plan_id + txid"
        }
    });

    if broadcast {
        let is_btc = chain.starts_with("bitcoin") || chain.contains("testnet3");
        let bc_val = if is_btc {
            let btc = lab_btc::BtcConfig::from_env();
            let utxos = lab_btc::utxos(cfg, &btc, wallet)?;
            let seal_val = utxos
                .iter()
                .find(|u| u.outpoint == issue.seal)
                .map(|u| u.value_sats)
                .context("seal UTXO not found in btc wallet")?;
            let fee = 800u64;
            let bc = lab_btc::broadcast_commitment_tx(
                cfg,
                &btc,
                wallet,
                &issue.seal,
                seal_val,
                &plan.tapret_address,
                commitment_sats,
                fee,
            )?;
            serde_json::to_value(bc)?
        } else {
            let bc = lab_chain::broadcast_commitment_tx(
                cfg,
                wallet,
                &issue.seal,
                &plan.tapret_address,
                bob_address.as_deref(),
                commitment_sats,
                bob_sats,
            )?;
            serde_json::to_value(bc)?
        };
        out["status"] = serde_json::json!("broadcast");
        out["broadcast"] = bc_val;
    }
    Ok(out)
}

/// POST /v1/audit/bfa — body is a BfaHistory JSON document (see docs/C3_CLOSED.md).
fn handle_bfa_audit_post(body: &str) -> Result<lab_rgb::bfa::BfaAuditResult> {
    let hist: lab_rgb::bfa::BfaHistory =
        serde_json::from_str(body).context("BFA history JSON")?;
    let fetch = |txid: &str| -> Result<String> {
        // Prefer embedded witness_tx_hex; if missing, try Elements regtest RPC helper.
        let out = std::process::Command::new("./scripts/regtest_simplicity.sh")
            .args(["cli", "getrawtransaction", txid])
            .output();
        match out {
            Ok(o) if o.status.success() => Ok(String::from_utf8_lossy(&o.stdout).trim().to_string()),
            _ => anyhow::bail!(
                "no witness_tx_hex for {txid} and regtest fetch failed (embed hex in history)"
            ),
        }
    };
    lab_rgb::bfa::audit_history(&hist, &fetch)
}

// ── S3 RGB-wrapped fund / claim helpers (CLI-first) ─────────────────────────

const LQ_POLICY_ASSET: &str = "144c654344aa716d6f3abcc1ca90e5641e4e2a7f633bc09fe3baf64585819a49";

fn claimer_p2wpkh_spk(label: &str) -> Result<(bitcoin::secp256k1::SecretKey, bitcoin::ScriptBuf, String)> {
    use bitcoin::key::{CompressedPublicKey, Secp256k1};
    use bitcoin::{Address, Network};
    let (sk, _) = htlc::demo_keypair(label)?;
    let secp = Secp256k1::new();
    let pk = bitcoin::secp256k1::PublicKey::from_secret_key(&secp, &sk);
    let compressed = CompressedPublicKey(pk);
    let dest = Address::p2wpkh(&compressed, Network::Testnet);
    Ok((sk, dest.script_pubkey(), dest.to_string()))
}

/// After value fund: plan transfer of full supply onto HTLC seal + broadcast BTC commitment.
fn s3_fund_wrap_btc(
    cfg: &Config,
    btc: &lab_btc::BtcConfig,
    rgb_store: &RgbStore,
    s: &mut lab_rgb::swap::SwapSession,
    commitment_sats: u64,
    entropy: u64,
) -> Result<serde_json::Value> {
    let cid = s
        .btc_contract_id
        .clone()
        .context("rgb_wrap BTC requires --btc-contract on init")?;
    let issue = rgb_store.load_issue(&cid)?;
    let fund_txid = s.btc_fund_txid.clone().context("btc fund txid")?;
    let fund_vout = s.btc_fund_vout.unwrap_or(0);
    let htlc_seal = format!("{fund_txid}:{fund_vout}");

    let plan = plan_transfer_to_seal(
        &issue.contract_id,
        issue.supply,
        issue.supply,
        &issue.seal,
        &htlc_seal,
        None,
        DEMO_INTERNAL_XONLY_HEX,
        entropy,
        &issue.ticker,
        "bitcoin-testnet",
    )?;
    let plan_id = format!(
        "s3-fund-btc-{}-{}",
        s.id,
        &plan.bundle_id_hex[..12.min(plan.bundle_id_hex.len())]
    );
    rgb_store.save_transfer(&plan_id, &plan)?;

    let utxos = lab_btc::utxos(cfg, btc, &s.alice_btc_wallet)?;
    let seal_val = utxos
        .iter()
        .find(|u| u.outpoint == issue.seal)
        .map(|u| u.value_sats)
        .context("BTC issue seal UTXO not found in alice wallet (issue then fund-wrap before spending seal)")?;
    let fee = 800u64;
    let bc = lab_btc::broadcast_commitment_tx(
        cfg,
        btc,
        &s.alice_btc_wallet,
        &issue.seal,
        seal_val,
        &plan.tapret_address,
        commitment_sats,
        fee,
    )?;

    // Best-effort verify once tx is visible.
    let mut fund_verify = None;
    if let Ok(w) = lab_btc::fetch_witness_for_rgb(btc, &bc.txid) {
        if let Ok(vr) = verify_against_witness(&plan, &w, &btc.explorer_base) {
            fund_verify = Some(vr.status.clone());
            let _ = rgb_store.save_proof(&format!("{plan_id}-fund"), &vr);
        }
    }

    if let Some(leg) = s.btc_rgb.as_mut() {
        leg.contract_id = issue.contract_id.clone();
        leg.amount = issue.supply;
        leg.issue_seal = Some(issue.seal.clone());
        leg.htlc_seal = Some(htlc_seal.clone());
        leg.fund_plan_id = Some(plan_id.clone());
        leg.fund_anchor_txid = Some(bc.txid.clone());
        leg.fund_verify = fund_verify.clone();
        leg.fund_transition_opid_hex = Some(plan.transition_opid_hex.clone());
    }
    s.notes
        .push(format!("S3 BTC fund-wrap plan={plan_id} seal={htlc_seal}"));

    Ok(serde_json::json!({
        "plan_id": plan_id,
        "htlc_seal": htlc_seal,
        "fund_anchor": bc,
        "fund_verify": fund_verify,
        "plan": plan,
    }))
}

fn s3_fund_wrap_lq(
    cfg: &Config,
    rgb_store: &RgbStore,
    s: &mut lab_rgb::swap::SwapSession,
    commitment_sats: u64,
    entropy: u64,
) -> Result<serde_json::Value> {
    let cid = s
        .lq_contract_id
        .clone()
        .context("rgb_wrap LQ requires --lq-contract on init")?;
    let issue = rgb_store.load_issue(&cid)?;
    let fund_txid = s.lq_fund_txid.clone().context("lq fund txid")?;
    let fund_vout = s.lq_fund_vout.unwrap_or(0);
    let htlc_seal = format!("{fund_txid}:{fund_vout}");

    let plan = plan_transfer_to_seal(
        &issue.contract_id,
        issue.supply,
        issue.supply,
        &issue.seal,
        &htlc_seal,
        None,
        DEMO_INTERNAL_XONLY_HEX,
        entropy,
        &issue.ticker,
        "liquid-testnet",
    )?;
    let plan_id = format!(
        "s3-fund-lq-{}-{}",
        s.id,
        &plan.bundle_id_hex[..12.min(plan.bundle_id_hex.len())]
    );
    rgb_store.save_transfer(&plan_id, &plan)?;

    let bc = lab_chain::broadcast_commitment_tx(
        cfg,
        &s.bob_lq_wallet,
        &issue.seal,
        &plan.tapret_address,
        None,
        commitment_sats,
        0,
    )?;

    let mut fund_verify = None;
    let api = lab_chain::esplora_api_base(cfg);
    if let Ok(w) = lab_chain::fetch_witness_esplora(&api, &bc.txid) {
        if let Ok(vr) = verify_against_witness(&plan, &w, &cfg.explorer_base) {
            fund_verify = Some(vr.status.clone());
            let _ = rgb_store.save_proof(&format!("{plan_id}-fund"), &vr);
        }
    }

    if let Some(leg) = s.lq_rgb.as_mut() {
        leg.contract_id = issue.contract_id.clone();
        leg.amount = issue.supply;
        leg.issue_seal = Some(issue.seal.clone());
        leg.htlc_seal = Some(htlc_seal.clone());
        leg.fund_plan_id = Some(plan_id.clone());
        leg.fund_anchor_txid = Some(bc.txid.clone());
        leg.fund_verify = fund_verify.clone();
        leg.fund_transition_opid_hex = Some(plan.transition_opid_hex.clone());
    }
    s.notes
        .push(format!("S3 LQ fund-wrap plan={plan_id} seal={htlc_seal}"));

    Ok(serde_json::json!({
        "plan_id": plan_id,
        "htlc_seal": htlc_seal,
        "fund_anchor": bc,
        "fund_verify": fund_verify,
        "plan": plan,
    }))
}

fn s3_claim_lq_value(
    cfg: &Config,
    s: &mut lab_rgb::swap::SwapSession,
    fee_sats: u64,
) -> Result<serde_json::Value> {
    let amount = s.lq_fund_sats.context("lq not funded (run fund-lq)")?;
    let (txid, vout, value) = lab_chain::find_address_utxo(
        cfg,
        &s.htlc_lq.address_liquid_unconf,
        amount.saturating_sub(1),
    )?;
    s.lq_fund_txid = Some(txid.clone());
    s.lq_fund_vout = Some(vout);
    s.lq_fund_sats = Some(value);

    let preimage = hex::decode(&s.preimage_hex)?;
    let (claimer_sk, dest_spk, dest_addr) = claimer_p2wpkh_spk(&s.htlc_lq.claimer_label)?;
    let ws = hex::decode(&s.htlc_lq.witness_script_hex)?;
    let out_sats = value.saturating_sub(fee_sats);
    let raw = htlc::build_htlc_spend_liquid(
        &txid,
        vout,
        value,
        out_sats,
        fee_sats,
        dest_spk.as_bytes(),
        LQ_POLICY_ASSET,
        &ws,
        htlc::HtlcSpend::Claim {
            preimage: &preimage,
        },
        s.csv_delay,
        &claimer_sk,
    )?;
    let claim_txid = lab_chain::broadcast_raw_hex(cfg, &raw)?;
    s.lq_claim_txid = Some(claim_txid.clone());
    Ok(serde_json::json!({
        "status": "claimed_lq",
        "phase": s.phase,
        "rgb_wrap": false,
        "txid": claim_txid,
        "dest": dest_addr,
        "explorer": format!("{}/tx/{}", cfg.explorer_base, claim_txid),
        "preimage_published": true,
        "note": "Preimage is public on Liquid; Bob can claim BTC.",
    }))
}

fn s3_claim_lq_rgb(
    cfg: &Config,
    rgb_store: &RgbStore,
    s: &mut lab_rgb::swap::SwapSession,
    fee_sats: u64,
    commitment_sats: u64,
    entropy: u64,
) -> Result<serde_json::Value> {
    let leg = s
        .lq_rgb
        .as_ref()
        .context("lq_rgb missing; fund-lq --rgb-wrap first")?
        .clone();
    let prev_opid = leg
        .fund_transition_opid_hex
        .clone()
        .context("fund_transition_opid_hex missing (fund-lq --rgb-wrap first)")?;
    let amount_rgb = if leg.amount > 0 {
        leg.amount
    } else {
        rgb_store.load_issue(&leg.contract_id)?.supply
    };

    let fund_amount = s.lq_fund_sats.context("lq not funded")?;
    let (txid, vout, value) = lab_chain::find_address_utxo(
        cfg,
        &s.htlc_lq.address_liquid_unconf,
        fund_amount.saturating_sub(1),
    )?;
    s.lq_fund_txid = Some(txid.clone());
    s.lq_fund_vout = Some(vout);
    s.lq_fund_sats = Some(value);
    // Prefer live seal if explorer disagrees with session.
    let htlc_seal = format!("{txid}:{vout}");

    // Layout: vout0 = tapret commitment, vout1 = claimer value (successor seal).
    let plan = plan_claim_transfer(
        &leg.contract_id,
        &prev_opid,
        0,
        amount_rgb,
        amount_rgb,
        &htlc_seal,
        1, // recipient WitnessTx vout
        None,
        DEMO_INTERNAL_XONLY_HEX,
        entropy,
        "lRGB",
        "liquid-testnet",
    )?;
    let plan_id = format!(
        "s3-claim-lq-{}-{}",
        s.id,
        &plan.bundle_id_hex[..12.min(plan.bundle_id_hex.len())]
    );
    rgb_store.save_transfer(&plan_id, &plan)?;

    let commit_spk = hex::decode(&plan.commitment_spk_hex)?;
    let preimage = hex::decode(&s.preimage_hex)?;
    let (claimer_sk, dest_spk, dest_addr) = claimer_p2wpkh_spk(&s.htlc_lq.claimer_label)?;
    let ws = hex::decode(&s.htlc_lq.witness_script_hex)?;
    if commitment_sats + fee_sats >= value {
        anyhow::bail!("commitment+fee must be < HTLC value");
    }
    let claimer_sats = value - commitment_sats - fee_sats;
    let raw = htlc::build_htlc_spend_liquid_outs(
        &txid,
        vout,
        value,
        &[
            (commitment_sats, commit_spk.as_slice()),
            (claimer_sats, dest_spk.as_bytes()),
        ],
        fee_sats,
        LQ_POLICY_ASSET,
        &ws,
        htlc::HtlcSpend::Claim {
            preimage: &preimage,
        },
        s.csv_delay,
        &claimer_sk,
    )?;
    let claim_txid = lab_chain::broadcast_raw_hex(cfg, &raw)?;
    s.lq_claim_txid = Some(claim_txid.clone());

    let mut claim_verify = None;
    let api = lab_chain::esplora_api_base(cfg);
    // Retry briefly — Esplora may lag.
    for _ in 0..5 {
        std::thread::sleep(std::time::Duration::from_millis(400));
        if let Ok(w) = lab_chain::fetch_witness_esplora(&api, &claim_txid) {
            if let Ok(vr) = verify_against_witness(&plan, &w, &cfg.explorer_base) {
                claim_verify = Some(vr.status.clone());
                let _ = rgb_store.save_proof(&format!("{plan_id}-claim"), &vr);
                break;
            }
        }
    }

    if let Some(r) = s.lq_rgb.as_mut() {
        r.htlc_seal = Some(htlc_seal.clone());
        r.claim_plan_id = Some(plan_id.clone());
        r.claim_anchor_txid = Some(claim_txid.clone());
        r.claim_verify = claim_verify.clone();
        r.successor_seal = Some(format!("{claim_txid}:1"));
    }
    s.notes.push(format!(
        "S3 LQ claim plan={plan_id} verify={claim_verify:?}"
    ));

    Ok(serde_json::json!({
        "status": "claimed_lq",
        "phase": s.phase,
        "rgb_wrap": true,
        "txid": claim_txid,
        "dest": dest_addr,
        "explorer": format!("{}/tx/{}", cfg.explorer_base, claim_txid),
        "preimage_published": true,
        "claim_plan_id": plan_id,
        "claim_verify": claim_verify,
        "successor_seal": format!("{claim_txid}:1"),
        "note": "Preimage public on Liquid; Bob can extract-preimage / claim-btc --from-witness.",
    }))
}

fn s3_claim_btc_value(
    cfg: &Config,
    s: &mut lab_rgb::swap::SwapSession,
    preimage: &[u8],
    fee_sats: u64,
) -> Result<serde_json::Value> {
    let btc = lab_btc::BtcConfig::from_env();
    let amount = s.btc_fund_sats.context("btc_fund_sats")?;
    let utxo = lab_btc::find_htlc_utxo(&btc, &s.htlc_btc.address_btc, amount.saturating_sub(1))?;
    let (claimer_sk, dest_spk, dest_addr) = claimer_p2wpkh_spk(&s.htlc_btc.claimer_label)?;
    let ws = hex::decode(&s.htlc_btc.witness_script_hex)?;
    let out_sats = utxo.value_sats.saturating_sub(fee_sats);
    let raw = htlc::build_htlc_spend_btc(
        &utxo.txid,
        utxo.vout,
        utxo.value_sats,
        out_sats,
        dest_spk.as_bytes(),
        &ws,
        htlc::HtlcSpend::Claim { preimage },
        s.csv_delay,
        &claimer_sk,
    )?;
    let txid = lab_btc::broadcast_raw(&btc, &raw)?;
    s.btc_claim_txid = Some(txid.clone());
    let _ = cfg;
    Ok(serde_json::json!({
        "status": "claimed_btc",
        "phase": s.phase,
        "rgb_wrap": false,
        "txid": txid,
        "dest": dest_addr,
        "explorer": format!("{}/tx/{}", btc.explorer_base, txid),
    }))
}

fn s3_claim_btc_rgb(
    cfg: &Config,
    rgb_store: &RgbStore,
    s: &mut lab_rgb::swap::SwapSession,
    preimage: &[u8],
    fee_sats: u64,
    commitment_sats: u64,
    entropy: u64,
) -> Result<serde_json::Value> {
    let btc = lab_btc::BtcConfig::from_env();
    let leg = s
        .btc_rgb
        .as_ref()
        .context("btc_rgb missing; fund-btc --rgb-wrap first")?
        .clone();
    let prev_opid = leg
        .fund_transition_opid_hex
        .clone()
        .context("fund_transition_opid_hex missing")?;
    let amount_rgb = if leg.amount > 0 {
        leg.amount
    } else {
        rgb_store.load_issue(&leg.contract_id)?.supply
    };
    let amount = s.btc_fund_sats.context("btc_fund_sats")?;
    let utxo = lab_btc::find_htlc_utxo(&btc, &s.htlc_btc.address_btc, amount.saturating_sub(1))?;
    let htlc_seal = format!("{}:{}", utxo.txid, utxo.vout);

    let plan = plan_claim_transfer(
        &leg.contract_id,
        &prev_opid,
        0,
        amount_rgb,
        amount_rgb,
        &htlc_seal,
        1,
        None,
        DEMO_INTERNAL_XONLY_HEX,
        entropy,
        "bRGB",
        "bitcoin-testnet",
    )?;
    let plan_id = format!(
        "s3-claim-btc-{}-{}",
        s.id,
        &plan.bundle_id_hex[..12.min(plan.bundle_id_hex.len())]
    );
    rgb_store.save_transfer(&plan_id, &plan)?;

    let commit_spk = hex::decode(&plan.commitment_spk_hex)?;
    let (claimer_sk, dest_spk, dest_addr) = claimer_p2wpkh_spk(&s.htlc_btc.claimer_label)?;
    let ws = hex::decode(&s.htlc_btc.witness_script_hex)?;
    if commitment_sats + fee_sats >= utxo.value_sats {
        anyhow::bail!("commitment+fee must be < HTLC value");
    }
    let claimer_sats = utxo.value_sats - commitment_sats - fee_sats;
    let raw = htlc::build_htlc_spend_btc_outs(
        &utxo.txid,
        utxo.vout,
        utxo.value_sats,
        &[
            (commitment_sats, commit_spk.as_slice()),
            (claimer_sats, dest_spk.as_bytes()),
        ],
        &ws,
        htlc::HtlcSpend::Claim { preimage },
        s.csv_delay,
        &claimer_sk,
    )?;
    let claim_txid = lab_btc::broadcast_raw(&btc, &raw)?;
    s.btc_claim_txid = Some(claim_txid.clone());

    let mut claim_verify = None;
    for _ in 0..5 {
        std::thread::sleep(std::time::Duration::from_millis(400));
        if let Ok(w) = lab_btc::fetch_witness_for_rgb(&btc, &claim_txid) {
            if let Ok(vr) = verify_against_witness(&plan, &w, &btc.explorer_base) {
                claim_verify = Some(vr.status.clone());
                let _ = rgb_store.save_proof(&format!("{plan_id}-claim"), &vr);
                break;
            }
        }
    }

    if let Some(r) = s.btc_rgb.as_mut() {
        r.htlc_seal = Some(htlc_seal);
        r.claim_plan_id = Some(plan_id.clone());
        r.claim_anchor_txid = Some(claim_txid.clone());
        r.claim_verify = claim_verify.clone();
        r.successor_seal = Some(format!("{claim_txid}:1"));
    }
    s.notes.push(format!(
        "S3 BTC claim plan={plan_id} verify={claim_verify:?}"
    ));
    let _ = cfg;

    Ok(serde_json::json!({
        "status": "claimed_btc",
        "phase": s.phase,
        "rgb_wrap": true,
        "txid": claim_txid,
        "dest": dest_addr,
        "explorer": format!("{}/tx/{}", btc.explorer_base, claim_txid),
        "claim_plan_id": plan_id,
        "claim_verify": claim_verify,
        "successor_seal": format!("{claim_txid}:1"),
    }))
}

fn resolve_preimage_from_lq_claim(cfg: &Config, s: &lab_rgb::swap::SwapSession) -> Result<Vec<u8>> {
    let txid = s
        .lq_claim_txid
        .as_ref()
        .context("no lq_claim_txid; claim-lq first or omit --from-witness")?;
    let pre = extract_preimage_cli(cfg, "liquid", txid)?;
    let hash = htlc::sha256_preimage(&pre);
    let session_hash = hex::decode(&s.hash_hex)?;
    if session_hash.as_slice() != hash.as_slice() {
        anyhow::bail!("extracted preimage hash does not match session hash_hex");
    }
    Ok(pre.to_vec())
}

fn extract_preimage_cli(cfg: &Config, chain: &str, txid: &str) -> Result<[u8; 32]> {
    let c = chain.trim().to_ascii_lowercase();
    if c.starts_with("bitcoin") || c == "btc" || c == "tb" {
        let btc = lab_btc::BtcConfig::from_env();
        let hex_tx = lab_btc::fetch_tx_hex(&btc, txid)?;
        htlc::extract_preimage_from_btc_tx_hex(&hex_tx)
    } else if c.starts_with("liquid") || c == "lq" || c == "elements" {
        let hex_tx = lab_chain::fetch_tx_hex(cfg, txid)?;
        htlc::extract_preimage_from_liquid_tx_hex(&hex_tx)
    } else {
        anyhow::bail!("chain must be bitcoin|btc or liquid|lq (got {chain})");
    }
}
