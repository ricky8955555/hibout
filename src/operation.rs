use std::{collections::HashMap, fs::File, io::Write, process::Stdio};

use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json;
use tera::{Context, Tera};
use tokio::{io::AsyncWriteExt, process::Command};
use tracing::{debug, error, info};

use crate::service::Handler;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Operation {
    Cmd { cmd: String, program: String },
    Script { template: String, program: String },
    File { dest: String, template: String },
}

pub type VarValue = Box<dyn erased_serde::Serialize + Send + Sync>;

pub struct Conductor {
    name: String,
    operations: Vec<Operation>,
    static_vars: HashMap<String, VarValue>,
}

impl Conductor {
    pub fn new(
        name: &str,
        operations: &[Operation],
        static_vars: HashMap<String, VarValue>,
    ) -> Self {
        Self {
            name: name.to_string(),
            operations: operations.to_vec(),
            static_vars,
        }
    }

    async fn run_cmd(cmd: &str, program: &str) -> Result<()> {
        debug!("running command {} via {}", cmd, program);

        let mut child = Command::new(program).stdin(Stdio::piped()).spawn()?;
        child
            .stdin
            .take()
            .unwrap()
            .write_all(cmd.as_bytes())
            .await?;
        child.wait().await?;

        Ok(())
    }

    async fn run_script(template: &str, program: &str, vars: impl Serialize) -> Result<()> {
        debug!(
            "running script via {} using template {} with vars {:?}",
            program,
            template,
            serde_json::to_string(&vars).unwrap_or("(unknown)".to_string()),
        );

        let mut tera = Tera::default();
        tera.add_template_file(template, None)?;
        let result = tera.render(template, &Context::from_serialize(vars)?)?;

        let mut child = Command::new(program).stdin(Stdio::piped()).spawn()?;
        child
            .stdin
            .take()
            .unwrap()
            .write_all(result.as_bytes())
            .await?;
        child.wait().await?;

        Ok(())
    }

    async fn write_file(dest: &str, template: &str, vars: impl Serialize) -> Result<()> {
        debug!(
            "writing file {} using template {} with vars {:?}",
            dest,
            template,
            serde_json::to_string(&vars).unwrap_or("(unknown)".to_string()),
        );

        let mut tera = Tera::default();
        tera.add_template_file(template, None)?;
        let result = tera.render(template, &Context::from_serialize(vars)?)?;
        File::create(dest)?.write_all(result.as_bytes())?;

        info!("wrote to {} with {} successfully", dest, template);
        Ok(())
    }
}

#[async_trait]
impl Handler for Conductor {
    async fn handle(&self, context: crate::service::Context) {
        let mut vars: HashMap<&str, &VarValue> = HashMap::from_iter(
            self.static_vars
                .iter()
                .map(|(key, value)| (key.as_str(), value)),
        );

        let loss = context.get_loss_count();
        let loss_rate = Box::new(loss as f64 / context.cycle as f64) as VarValue;
        let loss = Box::new(loss) as VarValue;
        let latencies = Box::new(context.latencies) as VarValue;
        let cycle = Box::new(context.cycle) as VarValue;

        vars.insert("loss", &loss);
        vars.insert("loss_rate", &loss_rate);
        vars.insert("latencies", &latencies);
        vars.insert("cycle", &cycle);

        debug!("{} is conducting operations", self.name);

        for operation in &self.operations {
            if let Err(e) = match operation {
                Operation::Cmd { cmd, program } => Self::run_cmd(cmd, program).await,
                Operation::Script { template, program } => {
                    Self::run_script(template, program, &vars).await
                }
                Operation::File { dest, template } => Self::write_file(dest, template, &vars).await,
            } {
                error!(
                    "{} failed when conducting operations. {}",
                    self.name, e
                );
                return; // immediately return if failed.
            }
        }

        info!("{} operations were successfully conducted", self.name);
    }
}
