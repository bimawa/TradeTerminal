use anyhow::Result;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Exchange {
    Bybit,
    Binance,
}

impl Exchange {
    pub fn from_str(s: &str) -> Result<Self> {
        match s.to_lowercase().as_str() {
            "bybit" => Ok(Exchange::Bybit),
            "binance" => Ok(Exchange::Binance),
            _ => anyhow::bail!("Unknown exchange: {}. Supported: bybit, binance", s),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Config {
    pub listen_addr: String,
    pub exchange: Exchange,
    pub bybit_api_key: String,
    pub bybit_api_secret: String,
    pub bybit_testnet: bool,
    pub binance_api_key: String,
    pub binance_api_secret: String,
    pub binance_testnet: bool,
    pub tls_enabled: bool,
    pub tls_cert_path: Option<String>,
    pub tls_key_path: Option<String>,
    pub auth_secret_key: Option<String>,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        let exchange_str = std::env::var("EXCHANGE").unwrap_or_else(|_| "bybit".into());
        let exchange = Exchange::from_str(&exchange_str)?;

        let bybit_api_key = std::env::var("BYBIT_API_KEY").unwrap_or_default();
        let bybit_api_secret = std::env::var("BYBIT_API_SECRET").unwrap_or_default();
        let binance_api_key = std::env::var("BINANCE_API_KEY").unwrap_or_default();
        let binance_api_secret = std::env::var("BINANCE_API_SECRET").unwrap_or_default();

        match exchange {
            Exchange::Bybit => {
                if bybit_api_key.is_empty() || bybit_api_secret.is_empty() {
                    anyhow::bail!("BYBIT_API_KEY and BYBIT_API_SECRET required when EXCHANGE=bybit");
                }
            }
            Exchange::Binance => {
                if binance_api_key.is_empty() || binance_api_secret.is_empty() {
                    anyhow::bail!("BINANCE_API_KEY and BINANCE_API_SECRET required when EXCHANGE=binance");
                }
            }
        }

        Ok(Self {
            listen_addr: std::env::var("LISTEN_ADDR").unwrap_or_else(|_| "0.0.0.0:9000".into()),
            exchange,
            bybit_api_key,
            bybit_api_secret,
            bybit_testnet: std::env::var("BYBIT_TESTNET")
                .map(|v| v == "true" || v == "1")
                .unwrap_or(false),
            binance_api_key,
            binance_api_secret,
            binance_testnet: std::env::var("BINANCE_TESTNET")
                .map(|v| v == "true" || v == "1")
                .unwrap_or(false),
            tls_enabled: std::env::var("TLS_ENABLED")
                .map(|v| v == "true" || v == "1")
                .unwrap_or(false),
            tls_cert_path: std::env::var("TLS_CERT_PATH").ok(),
            tls_key_path: std::env::var("TLS_KEY_PATH").ok(),
            auth_secret_key: std::env::var("AUTH_SECRET_KEY").ok(),
        })
    }

    pub fn rest_url(&self) -> &str {
        match self.exchange {
            Exchange::Bybit => {
                if self.bybit_testnet {
                    "https://api-testnet.bybit.com"
                } else {
                    "https://api.bybit.com"
                }
            }
            Exchange::Binance => {
                if self.binance_testnet {
                    "https://testnet.binancefuture.com"
                } else {
                    "https://fapi.binance.com"
                }
            }
        }
    }

    pub fn ws_url(&self) -> &str {
        match self.exchange {
            Exchange::Bybit => {
                if self.bybit_testnet {
                    "wss://stream-testnet.bybit.com/v5/private"
                } else {
                    "wss://stream.bybit.com/v5/private"
                }
            }
            Exchange::Binance => {
                if self.binance_testnet {
                    "wss://stream.binancefuture.com/ws"
                } else {
                    "wss://fstream.binance.com/ws"
                }
            }
        }
    }

    pub fn ws_public_url(&self) -> &str {
        match self.exchange {
            Exchange::Bybit => {
                if self.bybit_testnet {
                    "wss://stream-testnet.bybit.com/v5/public/linear"
                } else {
                    "wss://stream.bybit.com/v5/public/linear"
                }
            }
            Exchange::Binance => {
                if self.binance_testnet {
                    "wss://stream.binancefuture.com/ws"
                } else {
                    "wss://fstream.binance.com/ws"
                }
            }
        }
    }
}
