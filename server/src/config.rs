use anyhow::{Context, Result};

#[derive(Debug, Clone)]
pub struct Config {
    pub listen_addr: String,
    pub bybit_api_key: String,
    pub bybit_api_secret: String,
    pub bybit_testnet: bool,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        Ok(Self {
            listen_addr: std::env::var("LISTEN_ADDR").unwrap_or_else(|_| "0.0.0.0:9000".into()),
            bybit_api_key: std::env::var("BYBIT_API_KEY")
                .context("BYBIT_API_KEY environment variable required")?,
            bybit_api_secret: std::env::var("BYBIT_API_SECRET")
                .context("BYBIT_API_SECRET environment variable required")?,
            bybit_testnet: std::env::var("BYBIT_TESTNET")
                .map(|v| v == "true" || v == "1")
                .unwrap_or(false),
        })
    }

    pub fn rest_url(&self) -> &str {
        if self.bybit_testnet {
            "https://api-testnet.bybit.com"
        } else {
            "https://api.bybit.com"
        }
    }

    pub fn ws_url(&self) -> &str {
        if self.bybit_testnet {
            "wss://stream-testnet.bybit.com/v5/private"
        } else {
            "wss://stream.bybit.com/v5/private"
        }
    }

    pub fn ws_public_url(&self) -> &str {
        if self.bybit_testnet {
            "wss://stream-testnet.bybit.com/v5/public/linear"
        } else {
            "wss://stream.bybit.com/v5/public/linear"
        }
    }
}
