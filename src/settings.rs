use config::{Config, File};
use serde::Deserialize;
use anyhow::Result;
use walkdir::WalkDir;

use crate::operation::Operation;

#[derive(Clone, Debug, Deserialize)]
pub struct Service {
    pub name: String,
    pub interval: u64,  /* milliseconds */
    pub delta: u64,     /* milliseconds */
    pub cycle: usize,
    pub bind: String,
    pub dest: String,
    pub iface: String,
    pub operations: Vec<Operation>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Settings {
    pub instances: Vec<Service>,
}

impl Settings {
    pub fn new() -> Result<Self> {
        let mut instances = vec![];

        for file in WalkDir::new("config/instances/") {
            let file = file?;
            if file.file_type().is_file() {
                instances.push(
                    Config::builder()
                        .add_source(File::with_name("config/default/instance"))
                        .add_source(File::from(file.path()))
                        .build()?
                        .try_deserialize()?
                )
            }
        }

        Ok(Settings { instances })
    }
}
