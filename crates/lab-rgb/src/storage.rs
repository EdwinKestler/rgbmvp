//! On-disk RGB lab artifacts under `.rgbmvp/rgb/`.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use serde::{de::DeserializeOwned, Serialize};

use crate::{IssueResult, TransferPlan, VerifyResult};

#[derive(Debug, Clone)]
pub struct RgbStore {
    root: PathBuf,
}

impl RgbStore {
    pub fn new(data_dir: impl AsRef<Path>) -> Self {
        Self {
            root: data_dir.as_ref().join("rgb"),
        }
    }

    pub fn ensure(&self) -> Result<()> {
        fs::create_dir_all(self.root.join("contracts"))?;
        fs::create_dir_all(self.root.join("transfers"))?;
        fs::create_dir_all(self.root.join("consignments"))?;
        fs::create_dir_all(self.root.join("proofs"))?;
        Ok(())
    }

    fn contract_path(&self, contract_id: &str) -> PathBuf {
        let safe = contract_id.replace(['/', ':', ' '], "_");
        self.root.join("contracts").join(format!("{safe}.json"))
    }

    pub fn save_issue(&self, issue: &IssueResult) -> Result<PathBuf> {
        self.ensure()?;
        let path = self.contract_path(&issue.contract_id);
        write_json(&path, issue)?;
        Ok(path)
    }

    pub fn load_issue(&self, contract_id: &str) -> Result<IssueResult> {
        read_json(&self.contract_path(contract_id))
    }

    pub fn save_transfer(&self, id: &str, plan: &TransferPlan) -> Result<PathBuf> {
        self.ensure()?;
        let path = self.root.join("transfers").join(format!("{id}.json"));
        write_json(&path, plan)?;
        Ok(path)
    }

    pub fn load_transfer(&self, id: &str) -> Result<TransferPlan> {
        let path = self.root.join("transfers").join(format!("{id}.json"));
        if !path.exists() {
            // allow full path
            if Path::new(id).exists() {
                return read_json(Path::new(id));
            }
            bail!("transfer plan not found: {id}");
        }
        read_json(&path)
    }

    pub fn save_consignment_blob(&self, id: &str, bytes: &[u8]) -> Result<PathBuf> {
        self.ensure()?;
        let path = self.root.join("consignments").join(format!("{id}.bin"));
        fs::write(&path, bytes)?;
        Ok(path)
    }

    pub fn save_proof(&self, id: &str, proof: &VerifyResult) -> Result<PathBuf> {
        self.ensure()?;
        let path = self.root.join("proofs").join(format!("{id}.json"));
        write_json(&path, proof)?;
        Ok(path)
    }

    pub fn load_proof(&self, id: &str) -> Result<VerifyResult> {
        read_json(&self.root.join("proofs").join(format!("{id}.json")))
    }

    pub fn root(&self) -> &Path {
        &self.root
    }
}

fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    if let Some(p) = path.parent() {
        fs::create_dir_all(p)?;
    }
    let data = serde_json::to_vec_pretty(value)?;
    fs::write(path, data).with_context(|| format!("write {}", path.display()))?;
    Ok(())
}

fn read_json<T: DeserializeOwned>(path: &Path) -> Result<T> {
    let data = fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    Ok(serde_json::from_str(&data)?)
}
