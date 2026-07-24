//! Application services shared by CLI and labd (U5 preparation).
//!
//! Domain rules stay in `lab_rgb` / `lab_core`. This layer owns session I/O
//! orchestration that both surfaces can call without parsing Clap args.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use lab_rgb::swap::{self, SwapSession, SwapStore};

/// Swap session application service (init / load / save).
///
/// Fund/claim chain actions still live in `lab-cli` until full extraction;
/// callers that only need session lifecycle use this type.
#[derive(Debug, Clone)]
pub struct SwapService {
    store: PathBuf,
}

impl SwapService {
    pub fn new(data_dir: impl AsRef<Path>) -> Self {
        Self {
            store: data_dir.as_ref().to_path_buf(),
        }
    }

    pub fn store(&self) -> SwapStore {
        SwapStore::new(&self.store)
    }

    pub fn init(
        &self,
        id: &str,
        csv_delay: u32,
        alice_btc_wallet: &str,
        bob_lq_wallet: &str,
        btc_contract_id: Option<String>,
        lq_contract_id: Option<String>,
        rgb_wrap: bool,
    ) -> Result<SwapSession> {
        let session = swap::init_swap(
            id,
            csv_delay,
            alice_btc_wallet,
            bob_lq_wallet,
            btc_contract_id,
            lq_contract_id,
            rgb_wrap,
        )?;
        self.store()
            .save(&session)
            .context("save swap session")?;
        Ok(session)
    }

    pub fn load(&self, id: &str) -> Result<SwapSession> {
        self.store().load(id)
    }

    pub fn save(&self, session: &SwapSession) -> Result<PathBuf> {
        self.store().save(session)
    }

    pub fn exists(&self, id: &str) -> bool {
        self.store().path_exists(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn init_load_without_cli_args() {
        let dir = std::env::temp_dir().join(format!("rgbmvp-svc-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let svc = SwapService::new(&dir);
        let s = svc
            .init(
                "svc1",
                6,
                "btc-alice",
                "bob",
                Some("rgb:a".into()),
                Some("rgb:b".into()),
                true,
            )
            .unwrap();
        assert!(s.rgb_wrap);
        assert_eq!(s.version, 2);
        let loaded = svc.load("svc1").unwrap();
        assert_eq!(loaded.id, "svc1");
        assert_eq!(loaded.hash_hex, s.hash_hex);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
