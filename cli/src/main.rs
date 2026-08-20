use std::{env, path::PathBuf};

use anyhow::{Result, anyhow};
use cli::{
    config::Config,
    receiver::run_receiver,
    sender::{run_sender, ticket_from_env},
};

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    let config = Config::load()?;
    let role = env::args()
        .nth(1)
        .ok_or_else(|| anyhow!("expected 'receiver' or 'sender' as the first argument"))?;

    match role.as_str() {
        "receiver" => run_receiver(&config).await,
        "sender" => {
            let path = env::args().nth(2).ok_or_else(|| {
                anyhow!("expected a file or folder to send as the second argument")
            })?;
            run_sender(&config, ticket_from_env()?, &PathBuf::from(path)).await
        }
        _ => Err(anyhow!(
            "unknown role '{}'; use 'receiver' or 'sender'",
            role
        )),
    }
}
