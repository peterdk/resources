use anyhow::bail;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct LinkData {
    pub current_link: Option<String>,
    pub max_link: Option<String>,
    pub summary: Option<String>,
}

impl LinkData {
    pub fn for_disk(sysfs_path: &PathBuf) -> anyhow::Result<Self> {
        Self::get_pcie_speed(sysfs_path).or(Ok(LinkData {
            current_link: None,
            max_link: None,
            summary: None,
        }))
    }

    pub fn for_drm_gpu(sysfs_path: &PathBuf) -> anyhow::Result<Self> {
        return Self::read_pcie_speeds(sysfs_path).or(Ok(LinkData {
            current_link: None,
            max_link: None,
            summary: None,
        }));
    }
    pub fn get_pcie_speed(sysfs_path: &PathBuf) -> anyhow::Result<LinkData> {
        let path = sysfs_path.clone();
        if path.exists() {
            let address = std::fs::read_to_string(path.join("address"))?
                .trim()
                .to_string();
            if !address.is_empty() {
                return Self::get_pcie_speed_for_address(&address);
            }
        }
        bail!("Could not get PCIE link speed")
    }

    pub fn read_pcie_speeds(path: &PathBuf) -> anyhow::Result<LinkData> {
        let current_link_speed = std::fs::read_to_string(path.join("current_link_speed"))?
            .trim()
            .to_string();
        let max_link_speed = std::fs::read_to_string(path.join("max_link_speed"))?
            .trim()
            .to_string();
        let current_link_width = std::fs::read_to_string(path.join("current_link_width"))?
            .trim()
            .to_string();
        let max_link_width = std::fs::read_to_string(path.join("max_link_width"))?
            .trim()
            .to_string();

        let gt_pcie_map = HashMap::from(
            [
                ("2.5 GT/s PCIe", "1.0"),
                ("5.0 GT/s PCIe", "2.0"),
                ("8.0 GT/s PCIe", "3.0"),
                ("16.0 GT/s PCIe", "4.0"),
                ("32.0 GT/s PCIe", "5.0"),
            ]
            .map(|(k, v)| (k.to_lowercase(), v)),
        );
        let current_link_pcie = gt_pcie_map.get(&current_link_speed.to_lowercase());
        let max_link_pcie = gt_pcie_map.get(&max_link_speed.to_lowercase());

        if current_link_pcie.is_some() && max_link_pcie.is_some() {
            return Ok(LinkData {
                current_link: Some(format!(
                    "PCIe {} x{}",
                    current_link_pcie.unwrap().to_string(),
                    current_link_width,
                )),
                max_link: Some(format!(
                    "PCIe {} x{}",
                    max_link_pcie.unwrap().to_string(),
                    max_link_width
                )),
                summary: Some(format!(
                    "PCIe {} x{} / PCIe {} x{}",
                    current_link_pcie.unwrap().to_string(),
                    current_link_width,
                    max_link_pcie.unwrap().to_string(),
                    max_link_width
                )),
            });
        }
        bail!("Could not find PCIE link speed")
    }
    pub fn get_pcie_speed_for_address(address: &str) -> anyhow::Result<LinkData> {
        let pcie_dir = format!("/sys/bus/pci/devices/{address}/");
        let pcie_folder = Path::new(pcie_dir.as_str());
        if pcie_folder.exists() {
            return Self::read_pcie_speeds(&pcie_folder.to_path_buf());
        }
        bail!("Could not find PCIE speed")
    }
}
