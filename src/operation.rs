use std::{fs::File, io::Write, process::Stdio, collections::HashMap};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json;
use tera::{Tera, Context};
use tokio::{process::Command, io::AsyncWriteExt};
use anyhow::Result;
use tracing::{debug, info, error};

use crate::service::Handler;

#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "type")]
pub enum Operation {
    Cmd { cmd: String, program: String },
    Script { template: String, program: String },
    File { dest: String, template: String },
}

pub struct Conductor {
    name: String,
    operations: Vec<Operation>,
    static_vars: HashMap<String, Box<dyn erased_serde::Serialize + Send + Sync>>,
}

impl Conductor {
    pub fn new(name: &str, operations: &[Operation], static_vars: HashMap<String, Box<dyn erased_serde::Serialize + Send + Sync>>) -> Self {
        Self {
            name: name.to_string(),
            operations: operations.to_vec(),
            static_vars,
        }
    }

    async fn run_cmd(cmd: &str, program: &str) -> Result<()> {
        debug!(
            "running command {cmd} via {program}",
            cmd = cmd,
            program = program,
        );

        let mut child = Command::new(program).stdin(Stdio::piped()).spawn()?;
        child.stdin.take().unwrap().write_all(cmd.as_bytes()).await?;
        child.wait().await?;

        Ok(())
    }

    async fn run_script(template: &str, program: &str, vars: impl Serialize) -> Result<()> {
        debug!(
            "running script via {program} using template {template} with vars {vars:?}",
            program = program,
            template = template,
            vars = serde_json::to_string(&vars).unwrap_or("(unknown)".to_string()),
        );

        let mut tera = Tera::default();
        tera.add_template_file(template, None)?;
        let result = tera.render(template, &Context::from_serialize(vars)?)?;

        let mut child = Command::new(program).stdin(Stdio::piped()).spawn()?;
        child.stdin.take().unwrap().write_all(result.as_bytes()).await?;
        child.wait().await?;
        
        Ok(())
    }

    async fn write_file(dest: &str, template: &str, vars: impl Serialize) -> Result<()> {
        debug!(
            "writing file {file} using template {template} with vars {vars:?}",
            file = dest,
            template = template,
            vars = serde_json::to_string(&vars).unwrap_or("(unknown)".to_string()),
        );

        let mut tera = Tera::default();
        tera.add_template_file(template, None)?;
        let result = tera.render(template, &Context::from_serialize(vars)?)?;
        File::create(dest)?.write_all(result.as_bytes())?;

        info!("wrote to {file} with {template} successfully", file = dest, template = template);
        Ok(())
    }
}

#[async_trait]
impl Handler for Conductor {
    async fn handle(&self, context: crate::service::Context) {
        let mut vars: HashMap<String, &Box<dyn erased_serde::Serialize + Send + Sync>> = HashMap::from_iter(self.static_vars.iter().map(|(key, value)| (key.to_string(), value)));

        let loss = context.get_loss_count();
        let loss_rate = Box::new(loss as f64 / context.cycle as f64) as Box<dyn erased_serde::Serialize + Send + Sync>;
        let loss = Box::new(loss) as Box<dyn erased_serde::Serialize + Send + Sync>;
        let latencies = Box::new(context.latencies) as Box<dyn erased_serde::Serialize + Send + Sync>;
        let cycle = Box::new(context.cycle) as Box<dyn erased_serde::Serialize + Send + Sync>;

        vars.insert("loss".to_string(), &loss);
        vars.insert("loss_rate".to_string(), &loss_rate);
        vars.insert("latencies".to_string(), &latencies);
        vars.insert("cycle".to_string(), &cycle);

        debug!("{name} is conducting post operation", name = self.name);

        for operation in &self.operations {
            if let Err(e) = match operation {
                Operation::Cmd { cmd, program } => Self::run_cmd(&cmd, &program).await,
                Operation::Script { template, program } => Self::run_script(&template, &program, &vars).await,
                Operation::File { dest, template } => Self::write_file(&dest, &template, &vars).await,
            } {
                error!("{name} failed while conducting post operations. {error}", name = self.name, error = e);
                return;  // immediately return if failed.
            }
        }
    }
}
