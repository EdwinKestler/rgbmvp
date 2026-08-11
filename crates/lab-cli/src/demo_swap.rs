//! W1 — bounded public demo swap trigger (`POST /v1/demo/swap`).
//!
//! The public may *start* a swap; it may not shape one. Every protocol
//! parameter (amounts, fees, CSV delay, wallet names, RGB wrap) is fixed
//! server-side here. The granular `/v1/swap/{id}/action` endpoint accepts
//! arbitrary amounts and wallets and therefore stays token-gated and is never
//! reachable from this path.
//!
//! Admission and spending limits live in `lab_core::demo` (pure, tested).
//! This module supplies the three things that governor cannot do itself:
//! bot verification, on-chain float observation, and driving the swap.
//!
//! See `docs/TESTNET_PUBLIC_SWAPS.md` (ADR-T1).

use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use lab_core::{Config, DemoGovernor, Floats};
use serde_json::{json, Value};

/// How long an observed balance stays usable before a refresh is forced.
const FLOAT_TTL: Duration = Duration::from_secs(120);

/// Ceiling on how long the driver will babysit one swap before giving up.
const DRIVER_MAX_WALL: Duration = Duration::from_secs(60 * 90);

/// Delay between driver attempts while waiting for confirmations.
const DRIVER_POLL: Duration = Duration::from_secs(60);

/// Cloudflare Turnstile server-side verification endpoint.
const TURNSTILE_VERIFY_URL: &str =
    "https://challenges.cloudflare.com/turnstile/v0/siteverify";

pub fn now_epoch() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Wallet names the demo swaps between. Never visitor-supplied.
#[derive(Debug, Clone)]
pub struct DemoWallets {
    pub alice_btc: String,
    pub bob_lq: String,
}

impl DemoWallets {
    pub fn from_env() -> Self {
        Self {
            alice_btc: std::env::var("LABD_DEMO_BTC_WALLET")
                .unwrap_or_else(|_| "btc-alice".into()),
            bob_lq: std::env::var("LABD_DEMO_LQ_WALLET").unwrap_or_else(|_| "bob".into()),
        }
    }
}

/// Per-transaction fee the demo is willing to pay on each chain.
///
/// Defaults match the repo's own long-standing action defaults (fund_btc 800,
/// claim_btc 500), which were chosen from real runs. An earlier T1 draft used
/// ~200 sats derived from vbyte arithmetic; that is ~1.4 sat/vB and risks
/// sitting unconfirmed. Measure on a live run before lowering these.
///
/// The BTC leg spends two transactions (fund + claim), so a swap's BTC fee is
/// `btc_fund_fee_sats + btc_claim_fee_sats`; keep `LABD_DEMO_MAX_FEE_SATS`
/// at or above that sum.
#[derive(Debug, Clone, Copy)]
pub struct DemoFees {
    /// Fee for the BTC funding transaction.
    pub btc_fee_sats: u64,
    /// Fee for a Liquid demo-exit sweep transaction.
    pub lq_sweep_fee_sats: u64,
    /// Fee for the BTC claim/refund transaction.
    pub btc_claim_fee_sats: u64,
    pub lq_fee_sats: u64,
}

impl DemoFees {
    pub fn from_env() -> Self {
        Self {
            btc_fee_sats: env_u64("LABD_DEMO_BTC_FEE_SATS", 800),
            btc_claim_fee_sats: env_u64("LABD_DEMO_BTC_CLAIM_FEE_SATS", 500),
            lq_fee_sats: env_u64("LABD_DEMO_LQ_FEE_SATS", 300),
            lq_sweep_fee_sats: env_u64("LABD_DEMO_LQ_SWEEP_FEE_SATS", 400),
        }
    }

    /// Total BTC fee a completed swap burns (fund + claim).
    pub fn btc_total_per_swap(&self) -> u64 {
        self.btc_fee_sats + self.btc_claim_fee_sats
    }
}

fn env_u64(key: &str, default: u64) -> u64 {
    std::env::var(key)
        .ok()
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(default)
}

/// Caches observed wallet balances so a public request cannot trigger a slow
/// Electrum full-scan on every call.
///
/// Deliberately fail-closed: if the balances are missing or stale and a refresh
/// fails, callers receive `None` and the governor denies the swap rather than
/// spending against unknown funds.
#[derive(Debug)]
pub struct FloatCache {
    inner: Mutex<Option<(Floats, Instant)>>,
}

impl Default for FloatCache {
    fn default() -> Self {
        Self::new()
    }
}

impl FloatCache {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(None),
        }
    }

    fn get_fresh(&self) -> Option<Floats> {
        let guard = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        match *guard {
            Some((f, at)) if at.elapsed() < FLOAT_TTL => Some(f),
            _ => None,
        }
    }

    fn store(&self, f: Floats) {
        let mut guard = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        *guard = Some((f, Instant::now()));
    }

    /// Last observed floats regardless of age — for status display only, never
    /// for admission decisions.
    pub fn peek(&self) -> Option<Floats> {
        let guard = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        guard.map(|(f, _)| f)
    }

    /// Return fresh floats, refreshing from chain if the cache has expired.
    ///
    /// **Blocking** (Electrum full-scan + esplora): call inside `spawn_blocking`.
    pub fn observe_blocking(&self, cfg: &Config, wallets: &DemoWallets) -> Option<Floats> {
        if let Some(f) = self.get_fresh() {
            return Some(f);
        }
        match read_floats_blocking(cfg, wallets) {
            Ok(f) => {
                self.store(f);
                Some(f)
            }
            Err(e) => {
                eprintln!("demo: float refresh failed: {e}");
                None
            }
        }
    }
}

/// Read both funding wallet balances from chain.
fn read_floats_blocking(cfg: &Config, wallets: &DemoWallets) -> Result<Floats> {
    let btc_cfg = lab_btc::BtcConfig::from_env();
    btc_cfg
        .ensure_testnet()
        .context("demo swaps are testnet-only")?;
    let btc = lab_btc::balance(cfg, &btc_cfg, &wallets.alice_btc)
        .with_context(|| format!("btc balance for {}", wallets.alice_btc))?;
    let lq = lab_chain::wallet_balance(cfg, &wallets.bob_lq)
        .with_context(|| format!("liquid balance for {}", wallets.bob_lq))?;
    Ok(Floats {
        btc_sats: btc.balance_sats,
        lq_sats: lq.lbtc_sats,
    })
}

/// Outcome of a Turnstile check.
#[derive(Debug, PartialEq, Eq)]
pub enum BotCheck {
    /// Verified, or verification is not required on this deployment.
    Pass,
    /// No token supplied.
    Missing,
    /// Token rejected, or the check could not be completed (fail-closed).
    Failed,
}

/// Read the Turnstile secret from the environment.
///
/// Kept as a function rather than a stored field so the secret never lands in a
/// `Debug`-printable struct.
fn turnstile_secret() -> Option<String> {
    std::env::var("LABD_DEMO_TURNSTILE_SECRET")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// Verify a Cloudflare Turnstile token server-side.
///
/// **Blocking**: call inside `spawn_blocking`.
pub fn verify_turnstile_blocking(token: Option<&str>, remote_ip: Option<&str>) -> BotCheck {
    verify_turnstile_with(turnstile_secret().as_deref(), token, remote_ip)
}

/// Verification with an explicit secret, so callers (and tests) never depend on
/// ambient environment state.
pub fn verify_turnstile_with(
    secret: Option<&str>,
    token: Option<&str>,
    remote_ip: Option<&str>,
) -> BotCheck {
    // Required but unconfigured: refuse rather than silently allow.
    let secret = match secret.map(str::trim).filter(|s| !s.is_empty()) {
        Some(s) => s,
        None => {
            eprintln!("demo: turnstile required but LABD_DEMO_TURNSTILE_SECRET is unset");
            return BotCheck::Failed;
        }
    };
    let token = match token.map(str::trim).filter(|t| !t.is_empty()) {
        Some(t) => t,
        None => return BotCheck::Missing,
    };

    let client = match reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            eprintln!("demo: turnstile client build failed: {e}");
            return BotCheck::Failed;
        }
    };
    let mut form = vec![("secret", secret), ("response", token)];
    if let Some(ip) = remote_ip {
        form.push(("remoteip", ip));
    }
    match client.post(TURNSTILE_VERIFY_URL).form(&form).send() {
        Ok(resp) => match resp.json::<Value>() {
            Ok(v) if v.get("success").and_then(|s| s.as_bool()) == Some(true) => BotCheck::Pass,
            Ok(_) => BotCheck::Failed,
            Err(e) => {
                eprintln!("demo: turnstile response parse failed: {e}");
                BotCheck::Failed
            }
        },
        Err(e) => {
            eprintln!("demo: turnstile request failed: {e}");
            BotCheck::Failed
        }
    }
}

/// Deterministic, collision-resistant-enough demo swap id.
///
/// Uses epoch seconds plus a counter so ids stay sortable and match the
/// `[A-Za-z0-9._~-]` path-id rules enforced by `lab_core::is_safe_path_id`.
pub fn new_demo_swap_id(seq: u64) -> String {
    format!("demo-{}-{}", now_epoch(), seq)
}

/// Create the swap session with fully server-fixed parameters.
///
/// Returns the new swap id. Blocking (writes session state).
pub fn create_demo_session(
    cfg: &Config,
    wallets: &DemoWallets,
    seq: u64,
) -> Result<String> {
    let policy = lab_core::demo::DemoSwapPolicy::from_env();
    let svc = lab_api::SwapService::new(&cfg.data_dir);
    let id = new_demo_swap_id(seq);
    lab_core::validate_path_id(&id).context("generated demo swap id must be path-safe")?;
    svc.init(
        &id,
        policy.csv_delay,
        &wallets.alice_btc,
        &wallets.bob_lq,
        // No RGB contracts on the public demo: value-only path keeps the
        // footprint (and the dust) minimal.
        None,
        None,
        lab_core::demo::DEMO_RGB_WRAP,
    )
    .context("init demo swap session")?;
    Ok(id)
}

/// One step of the demo swap, with the exact payload the action handler needs.
fn driver_steps(leg_sats: u64, fees: DemoFees) -> Vec<(&'static str, Value)> {
    vec![
        (
            "fund_btc",
            json!({
                "action": "fund_btc",
                "amount_sats": leg_sats,
                "fee_sats": fees.btc_fee_sats,
                "rgb_wrap": false,
            }),
        ),
        (
            "fund_lq",
            json!({
                "action": "fund_lq",
                "amount_sats": leg_sats,
                "fee_sats": fees.lq_fee_sats,
                "rgb_wrap": false,
            }),
        ),
        (
            "claim_lq",
            json!({
                "action": "claim_lq",
                "fee_sats": fees.lq_fee_sats,
                "rgb_wrap": false,
            }),
        ),
        (
            "claim_btc",
            json!({
                "action": "claim_btc",
                "fee_sats": fees.btc_claim_fee_sats,
                "from_witness": true,
                "rgb_wrap": false,
            }),
        ),
    ]
}

/// Drive one demo swap to completion.
///
/// **Blocking and long-running** (waits on testnet confirmations): run on a
/// dedicated blocking task. Returns the BTC fee actually committed so the
/// governor can settle the budget.
///
/// Steps are idempotent in the underlying action handler (it refuses to
/// double-fund), so a retry after a transient failure resumes rather than
/// duplicates.
pub fn drive_demo_swap_blocking(
    cfg: &Config,
    swap_id: &str,
    leg_sats: u64,
    fees: DemoFees,
) -> Result<u64> {
    let store = lab_rgb::swap::SwapStore::new(&cfg.data_dir);
    let started = Instant::now();
    let mut btc_fee_committed = 0u64;

    for (name, payload) in driver_steps(leg_sats, fees) {
        let body = payload.to_string();
        loop {
            if started.elapsed() > DRIVER_MAX_WALL {
                anyhow::bail!("demo swap {swap_id} timed out at step {name}");
            }
            match crate::http_api::handle_swap_action_post(cfg, &store, swap_id, &body) {
                Ok(_) => {
                    match name {
                        "fund_btc" => btc_fee_committed += fees.btc_fee_sats,
                        "claim_btc" => btc_fee_committed += fees.btc_claim_fee_sats,
                        _ => {}
                    }
                    break;
                }
                Err(e) => {
                    // Most failures here are "not confirmed yet"; wait and retry
                    // until the wall-clock ceiling.
                    eprintln!("demo: swap {swap_id} step {name} pending/failed: {e}");
                    std::thread::sleep(DRIVER_POLL);
                }
            }
        }
    }
    Ok(btc_fee_committed)
}

// ---------------------------------------------------------------------------
// W4 — budget persistence
// ---------------------------------------------------------------------------

/// Where the fee-budget counters live. Beside the swap sessions, so a single
/// persistent volume covers both.
pub fn budget_path(cfg: &Config) -> std::path::PathBuf {
    cfg.data_dir.join("demo_budget.json")
}

/// Load persisted budget counters, if any.
///
/// A missing or unreadable file is not an error: the caller starts from zero.
/// A *corrupt* file is reported so the operator notices rather than silently
/// resetting the spend ceiling.
pub fn load_budget(cfg: &Config) -> Option<lab_core::DemoStatus> {
    let p = budget_path(cfg);
    let bytes = std::fs::read(&p).ok()?;
    match serde_json::from_slice::<lab_core::DemoStatus>(&bytes) {
        Ok(st) => Some(st),
        Err(e) => {
            eprintln!(
                "demo: budget file {} is unreadable ({e}); starting from zero \
                 — the fee ceiling for this run is effectively reset",
                p.display()
            );
            None
        }
    }
}

/// Persist budget counters atomically (write temp + rename), so a crash mid-write
/// cannot leave a truncated file that would silently reset the spend ceiling.
pub fn save_budget(cfg: &Config, st: &lab_core::DemoStatus) -> Result<()> {
    let p = budget_path(cfg);
    if let Some(dir) = p.parent() {
        std::fs::create_dir_all(dir).context("create data dir for demo budget")?;
    }
    let tmp = p.with_extension("json.tmp");
    let bytes = serde_json::to_vec_pretty(st).context("serialize demo budget")?;
    std::fs::write(&tmp, &bytes).context("write demo budget temp")?;
    std::fs::rename(&tmp, &p).context("rename demo budget into place")?;
    Ok(())
}

/// Snapshot the governor and persist it; logs rather than propagating, since a
/// persistence failure must not abort an otherwise healthy swap.
pub fn persist_budget(cfg: &Config, gov: &lab_core::DemoGovernor) {
    let st = gov.status(now_epoch());
    if let Err(e) = save_budget(cfg, &st) {
        eprintln!("demo: failed to persist budget: {e:#}");
    }
}

/// Restore persisted counters into a fresh governor at startup.
pub fn restore_budget(cfg: &Config, gov: &lab_core::DemoGovernor) {
    if let Some(st) = load_budget(cfg) {
        gov.restore(&st);
        eprintln!(
            "  T1 demo budget restored: spent={}sats swaps_total={} today={}",
            st.fee_spent_sats, st.swaps_total, st.swaps_today
        );
    }
}

// ---------------------------------------------------------------------------
// W5 — refund / recycle watcher
// ---------------------------------------------------------------------------

/// Default minimum age before a stuck swap is swept.
///
/// The HTLC refund path is consensus-gated on `csv_delay` confirmations, so
/// sweeping earlier just wastes a rejected broadcast. At ~10 min/block on BTC
/// testnet, csv=6 is ~60 min; 90 min leaves margin for slow blocks.
pub const DEFAULT_SWEEP_MIN_AGE_SECS: u64 = 90 * 60;

/// How often the watcher runs.
pub const DEFAULT_SWEEP_INTERVAL_SECS: u64 = 15 * 60;

/// Recover the start time encoded in a demo swap id (`demo-<epoch>-<seq>`).
///
/// `Some` also means "this id was minted by the demo endpoint". The sweeper
/// moves real funds, so it must never touch an operator's own swap session;
/// only ids matching this exact shape are eligible.
pub fn demo_swap_started_at(id: &str) -> Option<u64> {
    let rest = id.strip_prefix("demo-")?;
    let (epoch, seq) = rest.split_once('-')?;
    if epoch.is_empty() || seq.is_empty() {
        return None;
    }
    if !seq.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    epoch.parse::<u64>().ok()
}

/// `(interval_secs, min_age_secs)` for the refund watcher.
pub fn sweep_config_from_env() -> (u64, u64) {
    (
        env_u64("LABD_DEMO_SWEEP_INTERVAL_SECS", DEFAULT_SWEEP_INTERVAL_SECS).max(60),
        env_u64("LABD_DEMO_SWEEP_MIN_AGE_SECS", DEFAULT_SWEEP_MIN_AGE_SECS),
    )
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct SweepReport {
    pub scanned: usize,
    pub eligible: usize,
    pub refunded_btc: usize,
    pub refunded_lq: usize,
    pub skipped_young: usize,
    pub errors: usize,
    /// BTC sats recovered from demo exit addresses back into the funding wallet.
    pub recycled_sats: u64,
    /// L-BTC sats recovered from the Liquid demo exit addresses.
    pub recycled_lq_sats: u64,
}

/// Sweep the BTC demo exit addresses back into the funding wallet.
///
/// Runs after the refund pass: refunds land at `alice-refund` and completed
/// swaps at `bob-claimer`, neither of which is the funding wallet. Skips
/// silently when there is nothing to recover.
fn recycle_lq_exits_blocking(cfg: &Config, wallets: &DemoWallets, fee_sats: u64) -> u64 {
    match lab_chain::sweep_all_demo_exits_lq(cfg, &wallets.bob_lq, fee_sats) {
        Ok(results) => {
            let mut total = 0;
            for r in results {
                if let Some(txid) = &r.txid {
                    total += r.swept_sats;
                    eprintln!(
                        "demo recycle: swept {} L-BTC sats from {} -> {} ({txid})",
                        r.swept_sats, r.label, wallets.bob_lq
                    );
                }
            }
            total
        }
        Err(e) => {
            eprintln!("demo recycle: liquid sweep failed: {e:#}");
            0
        }
    }
}

fn recycle_btc_exits_blocking(cfg: &Config, wallets: &DemoWallets, fee_sats: u64) -> u64 {
    let btc = lab_btc::BtcConfig::from_env();
    if btc.ensure_testnet().is_err() {
        return 0;
    }
    match lab_btc::sweep_all_demo_exits(cfg, &btc, &wallets.alice_btc, fee_sats) {
        Ok(results) => {
            let mut total = 0;
            for r in results {
                if let Some(txid) = &r.txid {
                    total += r.swept_sats;
                    eprintln!(
                        "demo recycle: swept {} sats from {} -> {} ({txid})",
                        r.swept_sats, r.label, wallets.alice_btc
                    );
                }
            }
            total
        }
        Err(e) => {
            eprintln!("demo recycle: sweep failed: {e:#}");
            0
        }
    }
}

/// True when a session still has value parked in an HTLC.
fn needs_btc_refund(s: &lab_rgb::swap::SwapSession) -> bool {
    s.btc_fund_txid.is_some() && s.btc_claim_txid.is_none()
}

fn needs_lq_refund(s: &lab_rgb::swap::SwapSession) -> bool {
    s.lq_fund_txid.is_some() && s.lq_claim_txid.is_none()
}

/// Refund stuck demo swaps, then sweep the recovered value back to the funder.
///
/// IMPORTANT: an HTLC refund does **not** pay the funding wallet. Both refund
/// and claim paths pay a P2WPKH address derived from `demo_keypair(<label>)`
/// (`sha256(label)`) — four fixed addresses in total. Without the sweep below,
/// `btc-alice` drains on every swap regardless of outcome and the value strands
/// there. The keys are deterministic, so this is recovery, not rescue.
///
/// **Blocking and network-bound**: run on a blocking task. Failures are counted
/// and retried on the next sweep — a refund rejected because the CSV window has
/// not elapsed is expected, not an error condition.
pub fn sweep_stuck_demo_swaps_blocking(
    cfg: &Config,
    wallets: &DemoWallets,
    fees: DemoFees,
    min_age_secs: u64,
) -> SweepReport {
    let mut report = SweepReport::default();
    let ids = match crate::http_api::list_swap_ids(&cfg.data_dir) {
        Ok(ids) => ids,
        Err(e) => {
            eprintln!("demo sweep: cannot list swaps: {e:#}");
            report.errors += 1;
            return report;
        }
    };
    let store = lab_rgb::swap::SwapStore::new(&cfg.data_dir);
    let now = now_epoch();

    for id in ids {
        // Never touch operator sessions — only ids this module minted.
        let started = match demo_swap_started_at(&id) {
            Some(t) => t,
            None => continue,
        };
        report.scanned += 1;

        if now.saturating_sub(started) < min_age_secs {
            report.skipped_young += 1;
            continue;
        }
        let s = match store.load(&id) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("demo sweep: load {id}: {e:#}");
                report.errors += 1;
                continue;
            }
        };
        if matches!(
            s.phase,
            lab_rgb::swap::SwapPhase::Done | lab_rgb::swap::SwapPhase::Refunded
        ) {
            continue;
        }

        let btc = needs_btc_refund(&s);
        let lq = needs_lq_refund(&s);
        if !btc && !lq {
            continue;
        }
        report.eligible += 1;

        if lq {
            let body = json!({"action": "refund_lq", "fee_sats": fees.lq_fee_sats}).to_string();
            match crate::http_api::handle_swap_action_post(cfg, &store, &id, &body) {
                Ok(_) => {
                    report.refunded_lq += 1;
                    eprintln!("demo sweep: refunded liquid leg of {id}");
                }
                Err(e) => {
                    // Usually "CSV not elapsed" — retried next sweep.
                    eprintln!("demo sweep: refund_lq {id} pending/failed: {e}");
                    report.errors += 1;
                }
            }
        }
        if btc {
            let body =
                json!({"action": "refund_btc", "fee_sats": fees.btc_claim_fee_sats}).to_string();
            match crate::http_api::handle_swap_action_post(cfg, &store, &id, &body) {
                Ok(_) => {
                    report.refunded_btc += 1;
                    eprintln!("demo sweep: refunded bitcoin leg of {id}");
                }
                Err(e) => {
                    eprintln!("demo sweep: refund_btc {id} pending/failed: {e}");
                    report.errors += 1;
                }
            }
        }
    }

    // Recover value from the demo exit addresses back into the funding wallet.
    // Runs every sweep, not only when a refund fired: completed swaps also pay
    // out to `bob-claimer` and would otherwise strand there.
    report.recycled_sats = recycle_btc_exits_blocking(cfg, wallets, fees.btc_claim_fee_sats);
    report.recycled_lq_sats = recycle_lq_exits_blocking(cfg, wallets, fees.lq_sweep_fee_sats);
    report
}

/// Public JSON for `GET /v1/demo/quota`.
pub fn quota_json(gov: &DemoGovernor, floats: Option<Floats>) -> Value {
    let p = gov.policy();
    let st = gov.status(now_epoch());
    json!({
        "enabled": p.enabled,
        "network": "testnet",
        "leg_sats": p.leg_sats,
        "rgb_wrap": lab_core::demo::DEMO_RGB_WRAP,
        "csv_delay": p.csv_delay,
        "limits": {
            "daily_cap": p.daily_cap,
            "max_concurrent": p.max_concurrent,
            "min_interval_secs": p.global_min_interval_secs,
            "per_ip_hourly": p.per_ip_hourly,
            "per_ip_daily": p.per_ip_daily,
        },
        "budget": {
            "fee_budget_sats": p.fee_budget_sats,
            "fee_spent_sats": st.fee_spent_sats,
            "fee_reserved_sats": st.fee_reserved_sats,
            "swaps_remaining_est": gov.swaps_remaining_in_budget(),
        },
        "usage": {
            "in_flight": st.in_flight,
            "swaps_today": st.swaps_today,
            "swaps_total": st.swaps_total,
        },
        "floats": floats.map(|f| json!({
            "btc_sats": f.btc_sats,
            "lq_sats": f.lq_sats,
            "btc_floor_sats": p.btc_floor_sats,
            "lq_floor_sats": p.lq_floor_sats,
        })),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn demo_swap_ids_are_path_safe() {
        for seq in [0u64, 1, 42, 99_999] {
            let id = new_demo_swap_id(seq);
            assert!(
                lab_core::is_safe_path_id(&id),
                "generated id must satisfy the path-id rules: {id}"
            );
        }
    }

    /// The driver must never emit an RGB-wrapped or oversized leg.
    #[test]
    fn driver_steps_are_value_only_and_fixed() {
        let steps = driver_steps(1_000, DemoFees { btc_fee_sats: 800, btc_claim_fee_sats: 500, lq_fee_sats: 300, lq_sweep_fee_sats: 400 });
        assert_eq!(steps.len(), 4);
        for (name, payload) in &steps {
            assert_eq!(
                payload.get("rgb_wrap").and_then(|v| v.as_bool()),
                Some(false),
                "step {name} must stay on the value-only path"
            );
            assert!(
                payload.get("action").and_then(|v| v.as_str()).is_some(),
                "step {name} must carry an action"
            );
        }
        assert_eq!(
            steps[0].1.get("amount_sats").and_then(|v| v.as_u64()),
            Some(1_000)
        );
    }

    #[test]
    fn driver_steps_follow_htlc_order() {
        let steps = driver_steps(1_000, DemoFees { btc_fee_sats: 800, btc_claim_fee_sats: 500, lq_fee_sats: 300, lq_sweep_fee_sats: 400 });
        let names: Vec<&str> = steps.iter().map(|(n, _)| *n).collect();
        // Alice must claim Liquid (revealing the preimage) before Bob claims BTC.
        assert_eq!(names, vec!["fund_btc", "fund_lq", "claim_lq", "claim_btc"]);
    }

    /// Missing token is distinguishable from a rejected one.
    #[test]
    fn turnstile_missing_token_is_reported_as_missing() {
        let secret = Some("test-secret");
        assert_eq!(verify_turnstile_with(secret, None, None), BotCheck::Missing);
        assert_eq!(
            verify_turnstile_with(secret, Some("  "), None),
            BotCheck::Missing
        );
    }

    /// Fail closed when the verification secret is not configured: an
    /// unconfigured server must never wave traffic through.
    #[test]
    fn turnstile_without_secret_fails_closed() {
        assert_eq!(
            verify_turnstile_with(None, Some("some-token"), None),
            BotCheck::Failed
        );
        assert_eq!(
            verify_turnstile_with(Some("   "), Some("some-token"), None),
            BotCheck::Failed
        );
        // Fails closed even when no token is supplied either.
        assert_eq!(verify_turnstile_with(None, None, None), BotCheck::Failed);
    }

    #[test]
    fn float_cache_is_empty_until_observed() {
        let c = FloatCache::new();
        assert!(c.get_fresh().is_none());
        assert!(c.peek().is_none());
        c.store(Floats { btc_sats: 33_607, lq_sats: 146_633 });
        assert_eq!(c.get_fresh().unwrap().btc_sats, 33_607);
        assert_eq!(c.peek().unwrap().lq_sats, 146_633);
    }

    /// The sweeper refunds real funds, so its id filter is safety-critical:
    /// it must recognise only ids this module minted.
    /// Mirrors the sweeper's eligibility filter.
    fn sweepable(id: &str) -> bool {
        demo_swap_started_at(id).is_some()
    }

    #[test]
    fn sweeper_only_recognises_generated_demo_ids() {
        // Ours.
        assert_eq!(demo_swap_started_at("demo-1786400000-0"), Some(1786400000));
        assert_eq!(demo_swap_started_at("demo-1786400000-42"), Some(1786400000));
        assert!(sweepable("demo-1786400000-7"));

        // Pre-existing operator sessions in .rgbmvp/swaps must be ignored.
        for foreign in [
            "demo-u2",
            "demo-swap-1",
            "s3-demo",
            "p1-live",
            "u2-smoke",
            "s3-browser-20260724-0112",
            "s3-20260722-1251",
            "demo-",
            "demo-abc-1",
            "demo-123-",
            "demo-123-x",
        ] {
            assert!(
                !sweepable(foreign),
                "{foreign} must NOT be swept — it is not a generated demo id"
            );
        }
    }

    /// Every id the generator produces must be recognised by the sweeper,
    /// or stuck swaps would silently never be refunded.
    #[test]
    fn generated_ids_round_trip_through_the_sweeper_filter() {
        for seq in [0u64, 1, 9, 10, 12_345] {
            let id = new_demo_swap_id(seq);
            assert!(sweepable(&id), "generator/sweeper mismatch on {id}");
            assert!(lab_core::is_safe_path_id(&id));
        }
    }

    #[test]
    fn refund_eligibility_matches_htlc_state() {
        use lab_rgb::swap::init_swap;
        let mut s = init_swap("demo-1-0", 6, "btc-alice", "bob", None, None, false).unwrap();
        // Nothing funded yet: nothing to recover.
        assert!(!needs_btc_refund(&s));
        assert!(!needs_lq_refund(&s));

        s.btc_fund_txid = Some("aa".into());
        s.lq_fund_txid = Some("bb".into());
        assert!(needs_btc_refund(&s), "funded and unclaimed => refundable");
        assert!(needs_lq_refund(&s));

        // Once claimed, the value already moved; refunding would be wrong.
        s.btc_claim_txid = Some("cc".into());
        s.lq_claim_txid = Some("dd".into());
        assert!(!needs_btc_refund(&s));
        assert!(!needs_lq_refund(&s));
    }

    /// Budget must survive a restart, or a 2-week run silently overspends.
    #[test]
    fn budget_persists_across_restart() {
        let dir = std::env::temp_dir().join(format!("rgbmvp-demo-budget-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut cfg = Config::load().expect("config");
        cfg.data_dir = dir.clone();

        // Nothing persisted yet.
        assert!(load_budget(&cfg).is_none());

        let gov = lab_core::DemoGovernor::new(lab_core::DemoSwapPolicy {
            enabled: true,
            ..Default::default()
        });
        gov.try_admit(
            "1.1.1.1",
            now_epoch(),
            Some(lab_core::Floats { btc_sats: 33_607, lq_sats: 146_633 }),
        )
        .expect("admit");
        gov.finish(400);
        persist_budget(&cfg, &gov);

        // A fresh governor (simulating a restart) recovers the spend.
        let reloaded = load_budget(&cfg).expect("budget file written");
        assert_eq!(reloaded.fee_spent_sats, 400);
        assert_eq!(reloaded.swaps_total, 1);

        let gov2 = lab_core::DemoGovernor::new(lab_core::DemoSwapPolicy {
            enabled: true,
            ..Default::default()
        });
        restore_budget(&cfg, &gov2);
        let st = gov2.status(now_epoch());
        assert_eq!(st.fee_spent_sats, 400, "spend ceiling survived the restart");
        assert_eq!(st.in_flight, 0, "in-flight never survives a restart");
        // Derived from the constants so this cannot drift when fees change.
        let per_swap = lab_core::demo::DEFAULT_MAX_FEE_PER_SWAP_SATS;
        let budget = lab_core::demo::DEFAULT_FEE_BUDGET_SATS;
        assert_eq!(gov2.swaps_remaining_in_budget(), (budget - 400) / per_swap);

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A corrupt budget file must not crash startup (it degrades to zero, loudly).
    #[test]
    fn corrupt_budget_file_is_survivable() {
        let dir = std::env::temp_dir().join(format!("rgbmvp-demo-bad-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let mut cfg = Config::load().expect("config");
        cfg.data_dir = dir.clone();
        std::fs::write(budget_path(&cfg), b"{ this is not json").unwrap();
        assert!(load_budget(&cfg).is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// W7: these fields are what the ops alerts query. Renaming or dropping one
    /// silently blinds monitoring, so the contract is pinned here.
    #[test]
    fn quota_json_exposes_the_fields_ops_alerts_depend_on() {
        let gov = lab_core::DemoGovernor::new(lab_core::DemoSwapPolicy {
            enabled: true,
            ..Default::default()
        });
        let v = quota_json(
            &gov,
            Some(lab_core::Floats { btc_sats: 33_607, lq_sats: 146_633 }),
        );
        for path in [
            ("budget", "fee_spent_sats"),
            ("budget", "fee_budget_sats"),
            ("budget", "swaps_remaining_est"),
            ("usage", "in_flight"),
            ("usage", "swaps_today"),
            ("usage", "swaps_total"),
            ("floats", "btc_sats"),
            ("floats", "lq_sats"),
            ("floats", "btc_floor_sats"),
            ("floats", "lq_floor_sats"),
            ("limits", "daily_cap"),
            ("limits", "max_concurrent"),
        ] {
            assert!(
                v.get(path.0).and_then(|o| o.get(path.1)).is_some(),
                "ops alert field {}.{} is missing from /v1/demo/quota",
                path.0,
                path.1
            );
        }
    }

    /// Floats must be reported as null (not zero) when unknown, so a monitoring
    /// gap is never mistaken for an empty wallet.
    #[test]
    fn unknown_floats_report_null_not_zero() {
        let gov = lab_core::DemoGovernor::new(lab_core::DemoSwapPolicy {
            enabled: true,
            ..Default::default()
        });
        let v = quota_json(&gov, None);
        assert!(v["floats"].is_null(), "unknown floats must be null, not 0");
    }

    #[test]
    fn quota_json_reports_budget_and_flags() {
        let gov = DemoGovernor::new(lab_core::demo::DemoSwapPolicy {
            enabled: true,
            ..Default::default()
        });
        let v = quota_json(&gov, Some(Floats { btc_sats: 33_607, lq_sats: 146_633 }));
        assert_eq!(v["enabled"], json!(true));
        assert_eq!(v["rgb_wrap"], json!(false));
        assert_eq!(v["leg_sats"], json!(1_000));
        let expected = lab_core::demo::DEFAULT_FEE_BUDGET_SATS
            / lab_core::demo::DEFAULT_MAX_FEE_PER_SWAP_SATS;
        assert_eq!(v["budget"]["swaps_remaining_est"], json!(expected));
        // At repo-proven fees (800 fund + 500 claim) the run is ~21 swaps,
        // not the ~70 an earlier 400-sat estimate implied.
        assert_eq!(expected, 21);
        assert_eq!(v["floats"]["btc_sats"], json!(33_607));
    }
}
