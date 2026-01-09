use anyhow::{Context, Result};
use reqwest::Client;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};
use trade_shared::{Candle, Order, OrderRequest, OrderStatus, OrderType, Position, Side, Symbol, Ticker};

use super::sign::generate_signature;
use crate::config::Config;

const RECV_WINDOW: u64 = 5000;

#[derive(Clone)]
pub struct BybitClient {
    client: Client,
    api_key: String,
    api_secret: String,
    base_url: String,
}

#[derive(Debug, Deserialize)]
struct BybitResponse<T> {
    #[serde(default)]
    result: Option<T>,
}

#[derive(Debug, Deserialize)]
struct BybitErrorResponse {
    #[serde(rename = "retCode")]
    ret_code: i32,
    #[serde(rename = "retMsg")]
    ret_msg: String,
}

#[derive(Debug, Deserialize, Default)]
struct OrderResult {
    #[serde(rename = "orderId", default)]
    order_id: String,
}

#[derive(Debug, Deserialize, Default)]
struct OrderListResult {
    #[serde(default)]
    list: Vec<BybitOrder>,
}

#[derive(Debug, Deserialize)]
struct BybitOrder {
    #[serde(rename = "orderId")]
    order_id: String,
    symbol: String,
    side: String,
    #[serde(rename = "orderType")]
    order_type: String,
    qty: String,
    #[serde(rename = "cumExecQty")]
    cum_exec_qty: String,
    price: String,
    #[serde(rename = "avgPrice")]
    avg_price: String,
    #[serde(rename = "orderStatus")]
    order_status: String,
    #[serde(rename = "createdTime")]
    created_time: String,
}

#[derive(Debug, Deserialize, Default)]
struct PositionListResult {
    #[serde(default)]
    list: Vec<BybitPosition>,
}

#[derive(Debug, Deserialize)]
struct BybitPosition {
    symbol: String,
    side: String,
    size: String,
    #[serde(rename = "avgPrice")]
    avg_price: String,
    #[serde(rename = "unrealisedPnl")]
    unrealised_pnl: String,
    leverage: String,
    #[serde(rename = "takeProfit", default)]
    take_profit: String,
    #[serde(rename = "stopLoss", default)]
    stop_loss: String,
    #[serde(rename = "trailingStop", default)]
    trailing_stop: String,
}

#[derive(Debug, Deserialize, Default)]
struct TickerListResult {
    #[serde(default)]
    list: Vec<BybitTicker>,
}

#[derive(Debug, Deserialize)]
struct BybitTicker {
    #[serde(rename = "lastPrice")]
    last_price: String,
    #[serde(rename = "bid1Price")]
    bid_price: String,
    #[serde(rename = "ask1Price")]
    ask_price: String,
    #[serde(rename = "volume24h")]
    volume_24h: String,
    #[serde(rename = "price24hPcnt")]
    price_change_24h: String,
}

#[derive(Debug, Deserialize, Default)]
struct KlineListResult {
    #[serde(default)]
    list: Vec<Vec<String>>,
}

#[derive(Debug, Serialize)]
struct PlaceOrderRequest {
    category: String,
    symbol: String,
    side: String,
    #[serde(rename = "orderType")]
    order_type: String,
    qty: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    price: Option<String>,
    #[serde(rename = "timeInForce")]
    time_in_force: String,
    #[serde(rename = "positionIdx")]
    position_idx: u8,
    #[serde(rename = "reduceOnly")]
    reduce_only: bool,
    #[serde(rename = "takeProfit", skip_serializing_if = "Option::is_none")]
    take_profit: Option<String>,
    #[serde(rename = "stopLoss", skip_serializing_if = "Option::is_none")]
    stop_loss: Option<String>,
}

impl BybitClient {
    pub fn new(config: &Config) -> Self {
        Self {
            client: Client::new(),
            api_key: config.bybit_api_key.clone(),
            api_secret: config.bybit_api_secret.clone(),
            base_url: config.rest_url().to_string(),
        }
    }

    fn timestamp() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("Time went backwards")
            .as_millis() as u64
    }

    async fn post<T: for<'de> Deserialize<'de> + Default, B: Serialize>(&self, endpoint: &str, body: &B) -> Result<T> {
        let timestamp = Self::timestamp();
        let body_str = serde_json::to_string(body)?;
        let signature = generate_signature(&self.api_key, &self.api_secret, timestamp, RECV_WINDOW, &body_str);

        let url = format!("{}{}", self.base_url, endpoint);
        tracing::debug!("POST {} body: {}", url, body_str);
        
        let response = self
            .client
            .post(&url)
            .header("X-BAPI-API-KEY", &self.api_key)
            .header("X-BAPI-TIMESTAMP", timestamp.to_string())
            .header("X-BAPI-RECV-WINDOW", RECV_WINDOW.to_string())
            .header("X-BAPI-SIGN", signature)
            .header("Content-Type", "application/json")
            .body(body_str)
            .send()
            .await
            .context("Failed to send request")?;

        let resp_text = response.text().await.context("Failed to read response")?;
        tracing::debug!("GET Response (len={}): {}", resp_text.len(), resp_text);
        
        if resp_text.is_empty() {
            anyhow::bail!("Empty response from Bybit");
        }
        
        let err_resp: BybitErrorResponse = serde_json::from_str(&resp_text).context("Failed to parse response")?;
        if err_resp.ret_code != 0 {
            anyhow::bail!("Bybit error {}: {}", err_resp.ret_code, err_resp.ret_msg);
        }
        
        let resp: BybitResponse<T> = serde_json::from_str(&resp_text).context("Failed to parse result")?;
        resp.result.context("Empty result from Bybit")
    }

    async fn get<T: for<'de> Deserialize<'de> + Default>(&self, endpoint: &str, params: &str) -> Result<T> {
        let timestamp = Self::timestamp();
        let signature = generate_signature(&self.api_key, &self.api_secret, timestamp, RECV_WINDOW, params);

        let url = if params.is_empty() {
            format!("{}{}", self.base_url, endpoint)
        } else {
            format!("{}{}?{}", self.base_url, endpoint, params)
        };

        tracing::debug!("GET {}", url);
        
        let response = self
            .client
            .get(&url)
            .header("X-BAPI-API-KEY", &self.api_key)
            .header("X-BAPI-TIMESTAMP", timestamp.to_string())
            .header("X-BAPI-RECV-WINDOW", RECV_WINDOW.to_string())
            .header("X-BAPI-SIGN", signature)
            .send()
            .await
            .context("Failed to send request")?;

        let status = response.status();
        tracing::debug!("GET Response status: {}", status);
        
        let resp_text = response.text().await.context("Failed to read response")?;
        tracing::debug!("GET Response (len={}): '{}'", resp_text.len(), resp_text);
        
        if resp_text.is_empty() {
            anyhow::bail!("Empty response from Bybit");
        }
        
        let err_resp: BybitErrorResponse = serde_json::from_str(&resp_text).context("Failed to parse response")?;
        if err_resp.ret_code != 0 {
            anyhow::bail!("Bybit error {}: {}", err_resp.ret_code, err_resp.ret_msg);
        }
        
        let resp: BybitResponse<T> = serde_json::from_str(&resp_text).context("Failed to parse result")?;
        resp.result.context("Empty result from Bybit")
    }

    pub async fn place_order(&self, req: &OrderRequest) -> Result<Order> {
        let body = PlaceOrderRequest {
            category: "linear".to_string(),
            symbol: req.symbol.0.clone(),
            side: match req.side {
                Side::Buy => "Buy".to_string(),
                Side::Sell => "Sell".to_string(),
            },
            order_type: match req.order_type {
                OrderType::Market => "Market".to_string(),
                OrderType::Limit => "Limit".to_string(),
            },
            qty: req.quantity.to_string(),
            price: req.price.map(|p| p.to_string()),
            time_in_force: match req.time_in_force {
                trade_shared::TimeInForce::Gtc => "GTC".to_string(),
                trade_shared::TimeInForce::Ioc => "IOC".to_string(),
                trade_shared::TimeInForce::Fok => "FOK".to_string(),
                trade_shared::TimeInForce::PostOnly => "PostOnly".to_string(),
            },
            position_idx: req.position_idx.unwrap_or(match req.side {
                Side::Buy => 1,
                Side::Sell => 2,
            }),
            reduce_only: req.reduce_only,
            take_profit: req.take_profit.map(|p| p.to_string()),
            stop_loss: req.stop_loss.map(|p| p.to_string()),
        };

        let result: OrderResult = self.post("/v5/order/create", &body).await?;

        Ok(Order {
            id: result.order_id,
            symbol: req.symbol.clone(),
            side: req.side,
            order_type: req.order_type,
            quantity: req.quantity,
            filled_quantity: Decimal::ZERO,
            price: req.price,
            average_price: None,
            status: OrderStatus::New,
            created_at: chrono::Utc::now(),
        })
    }

    pub async fn cancel_order(&self, symbol: &Symbol, order_id: &str) -> Result<()> {
        #[derive(Serialize)]
        struct CancelRequest {
            category: String,
            symbol: String,
            #[serde(rename = "orderId")]
            order_id: String,
        }

        let body = CancelRequest {
            category: "linear".to_string(),
            symbol: symbol.0.clone(),
            order_id: order_id.to_string(),
        };

        let _: serde_json::Value = self.post("/v5/order/cancel", &body).await?;
        Ok(())
    }

    pub async fn cancel_all_orders(&self, symbol: Option<&Symbol>) -> Result<()> {
        #[derive(Serialize)]
        struct CancelAllRequest {
            category: String,
            #[serde(skip_serializing_if = "Option::is_none")]
            symbol: Option<String>,
        }

        let body = CancelAllRequest {
            category: "linear".to_string(),
            symbol: symbol.map(|s| s.0.clone()),
        };

        let _: serde_json::Value = self.post("/v5/order/cancel-all", &body).await?;
        Ok(())
    }

    pub async fn get_orders(&self, symbol: Option<&Symbol>) -> Result<Vec<Order>> {
        let params = match symbol {
            Some(s) => format!("category=linear&symbol={}", s.0),
            None => "category=linear&settleCoin=USDT".to_string(),
        };

        let result: OrderListResult = self.get("/v5/order/realtime", &params).await?;
        
        Ok(result.list.into_iter().map(convert_order).collect())
    }

    pub async fn get_positions(&self, symbol: Option<&Symbol>) -> Result<Vec<Position>> {
        let params = match symbol {
            Some(s) => format!("category=linear&symbol={}", s.0),
            None => "category=linear&settleCoin=USDT".to_string(),
        };

        let result: PositionListResult = self.get("/v5/position/list", &params).await?;

        tracing::debug!("Raw positions from Bybit: {} items", result.list.len());
        for (i, p) in result.list.iter().enumerate() {
            tracing::debug!("Position[{}]: symbol={} side={} size={}", i, p.symbol, p.side, p.size);
        }

        Ok(result
            .list
            .into_iter()
            .filter(|p| {
                let size = p.size.parse::<f64>().unwrap_or(0.0);
                tracing::debug!("Filtering position: symbol={} side={} size={} -> keep={}", p.symbol, p.side, p.size, size != 0.0);
                size != 0.0
            })
            .map(convert_position)
            .collect())
    }

    pub async fn close_position(&self, symbol: &Symbol, side: Side) -> Result<Order> {
        let positions = self.get_positions(Some(symbol)).await?;
        let position = positions
            .into_iter()
            .find(|p| p.symbol.0 == symbol.0 && p.side == side);

        let position = match position {
            Some(p) => p,
            None => anyhow::bail!("Position not found or already closed"),
        };

        let close_side = match side {
            Side::Buy => Side::Sell,
            Side::Sell => Side::Buy,
        };

        let position_idx = match position.side {
            Side::Buy => 1,
            Side::Sell => 2,
        };

        let req = trade_shared::OrderRequest {
            symbol: symbol.clone(),
            side: close_side,
            order_type: trade_shared::OrderType::Market,
            quantity: position.quantity,
            price: None,
            time_in_force: trade_shared::TimeInForce::Ioc,
            reduce_only: true,
            take_profit: None,
            stop_loss: None,
            position_idx: Some(position_idx),
        };

        self.place_order(&req).await
    }

    pub async fn get_ticker(&self, symbol: &Symbol) -> Result<Ticker> {
        let params = format!("category=linear&symbol={}", symbol.0);
        let result: TickerListResult = self.get_public("/v5/market/tickers", &params).await?;
        
        result
            .list
            .into_iter()
            .next()
            .map(|t| convert_ticker(t, symbol))
            .context("Ticker not found")
    }

    pub async fn get_klines(&self, symbol: &Symbol, interval: &str, limit: u32) -> Result<Vec<Candle>> {
        let params = format!(
            "category=linear&symbol={}&interval={}&limit={}",
            symbol.0, interval, limit
        );
        let result: KlineListResult = self.get_public("/v5/market/kline", &params).await?;

        let mut candles: Vec<Candle> = result
            .list
            .into_iter()
            .filter_map(|row| convert_kline(&row))
            .collect();

        candles.reverse();
        Ok(candles)
    }

    pub async fn set_trailing_stop(
        &self,
        symbol: &Symbol,
        side: Side,
        trailing_stop: Decimal,
        active_price: Option<Decimal>,
    ) -> Result<()> {
        #[derive(Serialize)]
        struct TradingStopRequest {
            category: String,
            symbol: String,
            #[serde(rename = "positionIdx")]
            position_idx: u8,
            #[serde(rename = "trailingStop")]
            trailing_stop: String,
            #[serde(rename = "activePrice", skip_serializing_if = "Option::is_none")]
            active_price: Option<String>,
        }

        let position_idx = match side {
            Side::Buy => 1,
            Side::Sell => 2,
        };

        let body = TradingStopRequest {
            category: "linear".to_string(),
            symbol: symbol.0.clone(),
            position_idx,
            trailing_stop: trailing_stop.to_string(),
            active_price: active_price.map(|p| p.to_string()),
        };

        let _: serde_json::Value = self.post("/v5/position/trading-stop", &body).await?;
        Ok(())
    }

    pub async fn get_position_mode(&self) -> Result<u8> {
        #[derive(Debug, Deserialize, Default)]
        struct PositionModeResult {
            list: Vec<PositionModeItem>,
        }

        #[derive(Debug, Deserialize)]
        struct PositionModeItem {
            #[serde(rename = "posMode")]
            pos_mode: u8,
        }

        let params = "category=linear";
        let result: PositionModeResult = self.get("/v5/position/get-position-mode", params).await?;

        result
            .list
            .into_iter()
            .next()
            .map(|item| item.pos_mode)
            .context("No position mode info returned")
    }

    pub async fn switch_to_hedge_mode(&self) -> Result<()> {
        #[derive(Serialize)]
        struct SwitchModeRequest {
            category: String,
            mode: u8,
        }

        let body = SwitchModeRequest {
            category: "linear".to_string(),
            mode: 3,
        };

        let _: serde_json::Value = self.post("/v5/position/switch-mode", &body).await?;
        tracing::info!("Switched to hedge mode (mode=3)");
        Ok(())
    }

    async fn get_public<T: for<'de> Deserialize<'de> + Default>(&self, endpoint: &str, params: &str) -> Result<T> {
        let url = format!("{}{}?{}", self.base_url, endpoint, params);
        tracing::debug!("GET (public) {}", url);

        let response = self
            .client
            .get(&url)
            .send()
            .await
            .context("Failed to send request")?;

        let resp_text = response.text().await.context("Failed to read response")?;
        tracing::debug!("Response: {}", resp_text);
        
        let err_resp: BybitErrorResponse = serde_json::from_str(&resp_text).context("Failed to parse response")?;
        if err_resp.ret_code != 0 {
            anyhow::bail!("Bybit error {}: {}", err_resp.ret_code, err_resp.ret_msg);
        }
        
        let resp: BybitResponse<T> = serde_json::from_str(&resp_text).context("Failed to parse result")?;
        resp.result.context("Empty result from Bybit")
    }
}

fn convert_order(o: BybitOrder) -> Order {
    Order {
        id: o.order_id,
        symbol: Symbol::new(o.symbol),
        side: if o.side == "Buy" { Side::Buy } else { Side::Sell },
        order_type: if o.order_type == "Market" { OrderType::Market } else { OrderType::Limit },
        quantity: o.qty.parse().unwrap_or_default(),
        filled_quantity: o.cum_exec_qty.parse().unwrap_or_default(),
        price: o.price.parse().ok(),
        average_price: o.avg_price.parse().ok(),
        status: match o.order_status.as_str() {
            "New" => OrderStatus::New,
            "PartiallyFilled" => OrderStatus::PartiallyFilled,
            "Filled" => OrderStatus::Filled,
            "Cancelled" => OrderStatus::Cancelled,
            _ => OrderStatus::Rejected,
        },
        created_at: chrono::DateTime::from_timestamp_millis(o.created_time.parse().unwrap_or(0))
            .unwrap_or_else(chrono::Utc::now),
    }
}

fn convert_position(p: BybitPosition) -> Position {
    let parse_opt = |s: &str| -> Option<Decimal> {
        if s.is_empty() || s == "0" {
            None
        } else {
            s.parse().ok()
        }
    };

    Position {
        symbol: Symbol::new(p.symbol),
        side: if p.side == "Buy" { Side::Buy } else { Side::Sell },
        quantity: p.size.parse().unwrap_or_default(),
        entry_price: p.avg_price.parse().unwrap_or_default(),
        unrealized_pnl: p.unrealised_pnl.parse().unwrap_or_default(),
        leverage: p.leverage.parse().unwrap_or(1),
        take_profit: parse_opt(&p.take_profit),
        stop_loss: parse_opt(&p.stop_loss),
        trailing_stop: parse_opt(&p.trailing_stop),
    }
}

fn convert_ticker(t: BybitTicker, symbol: &Symbol) -> Ticker {
    Ticker {
        symbol: symbol.clone(),
        last_price: t.last_price.parse().unwrap_or_default(),
        bid_price: t.bid_price.parse().unwrap_or_default(),
        ask_price: t.ask_price.parse().unwrap_or_default(),
        volume_24h: t.volume_24h.parse().unwrap_or_default(),
        price_change_24h: t.price_change_24h.parse().unwrap_or_default(),
    }
}

fn convert_kline(row: &[String]) -> Option<Candle> {
    if row.len() < 6 {
        return None;
    }
    Some(Candle {
        timestamp: row[0].parse().ok()?,
        open: row[1].parse().ok()?,
        high: row[2].parse().ok()?,
        low: row[3].parse().ok()?,
        close: row[4].parse().ok()?,
        volume: row[5].parse().ok()?,
    })
}
