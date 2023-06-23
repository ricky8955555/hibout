use std::{
    collections::HashMap,
    net::{IpAddr, SocketAddr},
    time::Duration,
};

use anyhow::Result;
use hibout::{operation::Conductor, service::Service, settings::Settings};
use tokio::time;
use tracing_subscriber::filter::{EnvFilter, LevelFilter};

fn parse_addr(addr: &str) -> Result<SocketAddr> {
    if let Ok(ip) = addr.parse::<IpAddr>() {
        return Ok(SocketAddr::new(ip, 2434));
    }
    Ok(addr.parse::<SocketAddr>()?)
}

#[tokio::main]
async fn main() -> Result<()> {
    let filter = EnvFilter::builder()
        .with_default_directive(if cfg!(debug_assertions) {
            LevelFilter::DEBUG.into()
        } else {
            LevelFilter::INFO.into()
        })
        .from_env_lossy();

    let subscriber = tracing_subscriber::fmt().with_env_filter(filter).finish();

    tracing::subscriber::set_global_default(subscriber)?;

    let settings = Settings::new()?;
    let mut services = vec![];

    for instance in settings.instances {
        let service = Service::new(&instance.name);
        let conductor = Conductor::new(&instance.name, &instance.operations, HashMap::new());

        service
            .run(
                parse_addr(&instance.bind)?,
                instance.iface.as_deref(),
                parse_addr(&instance.peer)?,
                Duration::from_millis(instance.interval),
                Duration::from_millis(instance.delta),
                instance.cycle,
                conductor,
            )
            .await?;
        services.push(service);
    }

    loop {
        time::sleep(Duration::from_secs(10)).await;
    }
}
