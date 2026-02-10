use anyhow::{Context, Result};
use reqwest::Client;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};
use trade_shared::{Candle, Order, OrderRequest, OrderStatus, OrderType, Position, Side, Symbol, Ticker, TrailingStopTarget};

use super::sign::generate_signature;
use crate::config::Config;

#[derive(Clone)]
pub struct BinanceClient {
    client: Client,
    api_key: String,
    api_secret: String,
    base_url: String,
}

#[derive(Debug, Deserialize)]
struct BinanceResponse<T> {
    #[serde(flatten)]
    result: T,
}

#[derive(Debug, Deserialize)]
struct BinanceError {
    code: i32,
    msg: String,
}

#[derive(Debug, Deserialize)]
struct OrderResult {
    #[serde(rename = "orderId")]
    order_id: i64,
}

#[derive(Debug, Deserialize)]
struct BinanceOrder {
    #[serde(rename = "orderId")]
    order_id: i64,
    symbol: String,
    side: String,
    #[serde(rename = "type")]
    order_type: String,
    #[serde(rename = "origQty")]
    orig_qty: String,
    #[serde(rename = "executedQty")]
    executed_qty: String,
    price: String,
    #[serde(rename = "avgPrice", default)]
    avg_price: String,
    status: String,
    time: i64,
}

#[derive(Debug, Deserialize)]
struct BinancePosition {
    symbol: String,
    #[serde(rename = "positionSide")]
    position_side: String,
    #[serde(rename = "positionAmt")]
    position_amt: String,
    #[serde(rename = "entryPrice")]
    entry_price: String,
    #[serde(rename = "unRealizedProfit")]
    unrealized_pnl: String,
    leverage: String,
}

#[derive(Debug, Deserialize)]
struct BinanceTicker {
    symbol: String,
    #[serde(rename = "lastPrice")]
    last_price: String,
    #[serde(rename = "bidPrice")]
    bid_price: String,
    #[serde(rename = "askPrice")]
    ask_price: String,
    volume: String,
    #[serde(rename = "priceChangePercent")]
    price_change_percent: String,
}

#[derive(Debug, Serialize)]
struct PlaceOrderRequest {
    symbol: String,
    side: String,
    #[serde(rename = "type")]
    order_type: String,
    quantity: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    price: Option<String>,
    #[serde(rename = "timeInForce", skip_serializing_if = "Option::is_none")]
    time_in_force: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "positionSide")]
    position_side: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "reduceOnly")]
    reduce_only: Option<String>,
    timestamp: u64,
}

impl BinanceClient {
    pub fn new(config: &Config) -> Self {
        Self {
            client: Client::new(),
            api_key: config.binance_api_key.clone(),
            api_secret: config.binance_api_secret.clone(),
            base_url: config.rest_url().to_string(),
        }
    }

    fn timestamp() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("Time went backwards")
            .as_millis() as u64
    }

    async fn signed_request<T: for<'de> Deserialize<'de>, B: Serialize>(
        &self,
        method: &str,
        endpoint: &str,
        params: &B,
    ) -> Result<T> {
        let timestamp = Self::timestamp();
        let mut query = serde_urlencoded::to_string(params)?;
        query.push_str(&format!("&timestamp={}", timestamp));

        let signature = generate_signature(&self.api_secret, &query);
        query.push_str(&format!("&signature={}", signature));

        let url = format!("{}{}", self.base_url, endpoint);
        tracing::debug!("{} {} query: {}", method, url, query);

        let response = match method {
            "POST" => {
                self.client
                    .post(&format!("{}?{}", url, query))
                    .header("X-MBX-APIKEY", &self.api_key)
                    .send()
                    .await
                    .context("Failed to send request")?
            }
            "DELETE" => {
                self.client
                    .delete(&format!("{}?{}", url, query))
                    .header("X-MBX-APIKEY", &self.api_key)
                    .send()
                    .await
                    .context("Failed to send request")?
            }
            _ => {
                self.client
                    .get(&format!("{}?{}", url, query))
                    .header("X-MBX-APIKEY", &self.api_key)
                    .send()
                    .await
                    .context("Failed to send request")?
            }
        };

        let status = response.status();
        let resp_text = response.text().await.context("Failed to read response")?;
        tracing::debug!("{} Response (status={}, len={}): {}", method, status, resp_text.len(), resp_text);

        if !status.is_success() {
            let err: BinanceError = serde_json::from_str(&resp_text)
                .context("Failed to parse error response")?;
            anyhow::bail!("Binance error {}: {}", err.code, err.msg);
        }

        serde_json::from_str(&resp_text).context("Failed to parse response")
    }

    pub async fn place_order(&self, req: &OrderRequest) -> Result<Order> {
        let position_side = match req.side {
            Side::Buy => "LONG",
            Side::Sell => "SHORT",
        };

        let order_type = match req.order_type {
            OrderType::Market => "MARKET",
            OrderType::Limit => "LIMIT",
        };

        let time_in_force = if req.order_type == OrderType::Limit {
            Some(match req.time_in_force {
                trade_shared::TimeInForce::Gtc => "GTC",
                trade_shared::TimeInForce::Ioc => "IOC",
                trade_shared::TimeInForce::Fok => "FOK",
                trade_shared::TimeInForce::PostOnly => "GTX",
            }.to_string())
        } else {
            None
        };

        #[derive(Serialize)]
        struct OrderParams {
            symbol: String,
            side: String,
            #[serde(rename = "type")]
            order_type: String,
            quantity: String,
            #[serde(skip_serializing_if = "Option::is_none")]
            price: Option<String>,
            #[serde(rename = "timeInForce", skip_serializing_if = "Option::is_none")]
            time_in_force: Option<String>,
            #[serde(rename = "positionSide")]
            position_side: String,
            #[serde(rename = "reduceOnly", skip_serializing_if = "Option::is_none")]
            reduce_only: Option<String>,
        }

        let params = OrderParams {
            symbol: req.symbol.0.clone(),
            side: match req.side {
                Side::Buy => "BUY".to_string(),
                Side::Sell => "SELL".to_string(),
            },
            order_type: order_type.to_string(),
            quantity: req.quantity.to_string(),
            price: req.price.map(|p| p.to_string()),
            time_in_force,
            position_side: position_side.to_string(),
            reduce_only: if req.reduce_only { Some("true".to_string()) } else { None },
        };

        let result: OrderResult = self.signed_request("POST", "/fapi/v1/order", &params).await?;

        Ok(Order {
            id: result.order_id.to_string(),
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
        struct CancelParams {
            symbol: String,
            #[serde(rename = "orderId")]
            order_id: String,
        }

        let params = CancelParams {
            symbol: symbol.0.clone(),
            order_id: order_id.to_string(),
        };

        let _: serde_json::Value = self.signed_request("DELETE", "/fapi/v1/order", &params).await?;
        Ok(())
    }

    pub async fn cancel_all_orders(&self, symbol: Option<&Symbol>) -> Result<()> {
        #[derive(Serialize)]
        struct CancelAllParams {
            symbol: String,
        }

        let symbol_str = symbol.map(|s| s.0.clone()).unwrap_or_else(|| "".to_string());
        if symbol_str.is_empty() {
            anyhow::bail!("Binance requires symbol for cancel all orders");
        }

        let params = CancelAllParams {
            symbol: symbol_str,
        };

        let _: serde_json::Value = self.signed_request("DELETE", "/fapi/v1/allOpenOrders", &params).await?;
        Ok(())
    }

    pub async fn get_orders(&self, symbol: Option<&Symbol>) -> Result<Vec<Order>> {
        #[derive(Serialize)]
        struct OrderParams {
            #[serde(skip_serializing_if = "Option::is_none")]
            symbol: Option<String>,
        }

        let params = OrderParams {
            symbol: symbol.map(|s| s.0.clone()),
        };

        let orders: Vec<BinanceOrder> = self.signed_request("GET", "/fapi/v1/openOrders", &params).await?;
        Ok(orders.into_iter().map(convert_order).collect())
    }

    pub async fn get_positions(&self, symbol: Option<&Symbol>) -> Result<Vec<Position>> {
        #[derive(Serialize)]
        struct PositionParams {}

        let params = PositionParams {};
        let positions: Vec<BinancePosition> = self.signed_request("GET", "/fapi/v2/positionRisk", &params).await?;

        tracing::debug!("Raw positions from Binance: {} items", positions.len());

        let filtered: Vec<Position> = positions
            .into_iter()
            .filter(|p| {
                let amt = p.position_amt.parse::<f64>().unwrap_or(0.0);
                let matches_symbol = symbol.map_or(true, |s| p.symbol == s.0);
                tracing::debug!("Position: symbol={} side={} amt={} -> keep={}",
                    p.symbol, p.position_side, p.position_amt, amt != 0.0 && matches_symbol);
                amt != 0.0 && matches_symbol
            })
            .map(convert_position)
            .collect();

        Ok(filtered)
    }

    pub async fn close_position(&self, symbol: &Symbol, side: Side) -> Result<Order> {
        let positions = self.get_positions(Some(symbol)).await?;
        let position = positions
            .into_iter()
            .find(|p| p.symbol.0 == symbol.0 && p.side == side)
            .context("Position not found or already closed")?;

        let close_side = match side {
            Side::Buy => Side::Sell,
            Side::Sell => Side::Buy,
        };

        let req = OrderRequest {
            symbol: symbol.clone(),
            side: close_side,
            order_type: OrderType::Market,
            quantity: position.quantity,
            price: None,
            time_in_force: trade_shared::TimeInForce::Gtc,
            reduce_only: true,
            take_profit: None,
            stop_loss: None,
            position_idx: None,
        };

        self.place_order(&req).await
    }

    pub async fn get_ticker(&self, symbol: &Symbol) -> Result<Ticker> {
        let url = format!("{}/fapi/v1/ticker/24hr?symbol={}", self.base_url, symbol.0);
        tracing::debug!("GET (public) {}", url);

        let response = self.client.get(&url).send().await.context("Failed to send request")?;
        let resp_text = response.text().await.context("Failed to read response")?;

        let ticker: BinanceTicker = serde_json::from_str(&resp_text).context("Failed to parse ticker")?;
        Ok(convert_ticker(ticker, symbol))
    }

    pub async fn get_klines(&self, symbol: &Symbol, interval: &str, limit: u32) -> Result<Vec<Candle>> {
        let url = format!(
            "{}/fapi/v1/klines?symbol={}&interval={}&limit={}",
            self.base_url, symbol.0, interval, limit
        );
        tracing::debug!("GET (public) {}", url);

        let response = self.client.get(&url).send().await.context("Failed to send request")?;
        let resp_text = response.text().await.context("Failed to read response")?;

        let klines: Vec<Vec<serde_json::Value>> = serde_json::from_str(&resp_text)
            .context("Failed to parse klines")?;

        Ok(klines.into_iter().filter_map(convert_kline).collect())
    }

    async fn resolve_trailing_stop_target(
        &self,
        symbol: &Symbol,
        side: Side,
        target: TrailingStopTarget,
    ) -> Result<Decimal> {
        match target {
            TrailingStopTarget::Absolute(value) => Ok(value),
            TrailingStopTarget::Percentage(pct) => {
                let ticker = self.get_ticker(symbol).await?;
                Ok(ticker.last_price * pct / Decimal::from(100))
            }
            TrailingStopTarget::Ratio { numerator, denominator } => {
                let positions = self.get_positions(Some(symbol)).await?;
                let position = positions
                    .iter()
                    .find(|p| p.side == side)
                    .context("Position not found for this symbol and side")?;

                let stop_loss = position.stop_loss
                    .context("Stop loss not set, cannot calculate ratio target")?;

                let sl_distance = (stop_loss - position.entry_price).abs();
                let multiplier = Decimal::from(denominator) / Decimal::from(numerator);
                let target_distance = sl_distance * multiplier;

                let target_price = match side {
                    Side::Buy => position.entry_price + target_distance,
                    Side::Sell => position.entry_price - target_distance,
                };

                Ok(target_price)
            }
        }
    }

    pub async fn set_trailing_stop(
        &self,
        symbol: &Symbol,
        side: Side,
        target: TrailingStopTarget,
        active_price: Option<Decimal>,
    ) -> Result<()> {
        let trailing_stop = self.resolve_trailing_stop_target(symbol, side, target).await?;

        #[derive(Serialize)]
        struct TrailingStopParams {
            symbol: String,
            side: String,
            #[serde(rename = "type")]
            order_type: String,
            #[serde(rename = "callbackRate")]
            callback_rate: String,
            #[serde(skip_serializing_if = "Option::is_none")]
            #[serde(rename = "activationPrice")]
            activation_price: Option<String>,
        }

        let params = TrailingStopParams {
            symbol: symbol.0.clone(),
            side: match side {
                Side::Buy => "SELL".to_string(),
                Side::Sell => "BUY".to_string(),
            },
            order_type: "TRAILING_STOP_MARKET".to_string(),
            callback_rate: trailing_stop.to_string(),
            activation_price: active_price.map(|p| p.to_string()),
        };

        let _: serde_json::Value = self.signed_request("POST", "/fapi/v1/order", &params).await?;
        Ok(())
    }

    pub async fn switch_to_hedge_mode(&self) -> Result<()> {
        #[derive(Serialize)]
        struct HedgeModeParams {
            #[serde(rename = "dualSidePosition")]
            dual_side: String,
        }

        let params = HedgeModeParams {
            dual_side: "true".to_string(),
        };

        let _: serde_json::Value = self.signed_request("POST", "/fapi/v1/positionSide/dual", &params).await?;
        tracing::info!("Switched to hedge mode (dualSidePosition=true)");
        Ok(())
    }
}

fn convert_order(o: BinanceOrder) -> Order {
    Order {
        id: o.order_id.to_string(),
        symbol: Symbol::new(o.symbol),
        side: if o.side == "BUY" { Side::Buy } else { Side::Sell },
        order_type: if o.order_type == "MARKET" { OrderType::Market } else { OrderType::Limit },
        quantity: o.orig_qty.parse().unwrap_or_default(),
        filled_quantity: o.executed_qty.parse().unwrap_or_default(),
        price: o.price.parse().ok(),
        average_price: if !o.avg_price.is_empty() && o.avg_price != "0" {
            o.avg_price.parse().ok()
        } else {
            None
        },
        status: match o.status.as_str() {
            "NEW" => OrderStatus::New,
            "PARTIALLY_FILLED" => OrderStatus::PartiallyFilled,
            "FILLED" => OrderStatus::Filled,
            "CANCELED" => OrderStatus::Cancelled,
            _ => OrderStatus::Rejected,
        },
        created_at: chrono::DateTime::from_timestamp_millis(o.time)
            .unwrap_or_else(chrono::Utc::now),
    }
}

fn convert_position(p: BinancePosition) -> Position {
    let side = if p.position_side == "LONG" {
        Side::Buy
    } else {
        Side::Sell
    };

    Position {
        symbol: Symbol::new(p.symbol),
        side,
        quantity: p.position_amt.parse::<Decimal>().unwrap_or_default().abs(),
        entry_price: p.entry_price.parse().unwrap_or_default(),
        unrealized_pnl: p.unrealized_pnl.parse().unwrap_or_default(),
        leverage: p.leverage.parse().unwrap_or(1),
        take_profit: None,
        stop_loss: None,
        trailing_stop: None,
    }
}

fn convert_ticker(t: BinanceTicker, symbol: &Symbol) -> Ticker {
    Ticker {
        symbol: symbol.clone(),
        last_price: t.last_price.parse().unwrap_or_default(),
        bid_price: t.bid_price.parse().unwrap_or_default(),
        ask_price: t.ask_price.parse().unwrap_or_default(),
        volume_24h: t.volume.parse().unwrap_or_default(),
        price_change_24h: t.price_change_percent.parse().unwrap_or_default(),
    }
}

fn convert_kline(row: Vec<serde_json::Value>) -> Option<Candle> {
    if row.len() < 6 {
        return None;
    }
    Some(Candle {
        timestamp: row[0].as_i64()?,
        open: row[1].as_str()?.parse().ok()?,
        high: row[2].as_str()?.parse().ok()?,
        low: row[3].as_str()?.parse().ok()?,
        close: row[4].as_str()?.parse().ok()?,
        volume: row[5].as_str()?.parse().ok()?,
    })
}
