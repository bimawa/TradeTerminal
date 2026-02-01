use rust_decimal::Decimal;
use std::collections::BTreeMap;
use trade_shared::{Candle, Trade};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AggregationInterval {
    Sec1,
    Sec5,
    Sec10,
    Sec30,
}

impl AggregationInterval {
    pub fn seconds(&self) -> i64 {
        match self {
            AggregationInterval::Sec1 => 1,
            AggregationInterval::Sec5 => 5,
            AggregationInterval::Sec10 => 10,
            AggregationInterval::Sec30 => 30,
        }
    }

    pub fn to_interval_string(&self) -> String {
        match self {
            AggregationInterval::Sec1 => "1s".to_string(),
            AggregationInterval::Sec5 => "5s".to_string(),
            AggregationInterval::Sec10 => "10s".to_string(),
            AggregationInterval::Sec30 => "30s".to_string(),
        }
    }

    pub fn from_interval_string(s: &str) -> Option<Self> {
        match s {
            "1s" => Some(AggregationInterval::Sec1),
            "5s" => Some(AggregationInterval::Sec5),
            "10s" => Some(AggregationInterval::Sec10),
            "30s" => Some(AggregationInterval::Sec30),
            _ => None,
        }
    }
}

struct CandleBuilder {
    timestamp: i64,
    open: Decimal,
    high: Decimal,
    low: Decimal,
    close: Decimal,
    volume: Decimal,
}

impl CandleBuilder {
    fn new(trade: &Trade, interval_start: i64) -> Self {
        Self {
            timestamp: interval_start,
            open: trade.price,
            high: trade.price,
            low: trade.price,
            close: trade.price,
            volume: trade.qty,
        }
    }

    fn update(&mut self, trade: &Trade) {
        self.high = self.high.max(trade.price);
        self.low = self.low.min(trade.price);
        self.close = trade.price;
        self.volume += trade.qty;
    }

    fn to_candle(self) -> Candle {
        Candle {
            timestamp: self.timestamp,
            open: self.open,
            high: self.high,
            low: self.low,
            close: self.close,
            volume: self.volume,
        }
    }
}

pub struct TickAggregator {
    interval: AggregationInterval,
    current_builder: Option<CandleBuilder>,
    completed_candles: BTreeMap<i64, Candle>,
    max_candles: usize,
}

impl TickAggregator {
    pub fn new(interval: AggregationInterval) -> Self {
        Self {
            interval,
            current_builder: None,
            completed_candles: BTreeMap::new(),
            max_candles: 500,
        }
    }

    fn get_interval_start(&self, timestamp_ms: i64) -> i64 {
        let seconds = self.interval.seconds();
        let timestamp_sec = timestamp_ms / 1000;
        (timestamp_sec / seconds) * seconds * 1000
    }

    pub fn add_trade(&mut self, trade: &Trade) -> Option<Candle> {
        let interval_start = self.get_interval_start(trade.timestamp);

        let mut completed_candle = None;

        if let Some(builder) = &mut self.current_builder {
            if builder.timestamp == interval_start {
                builder.update(trade);
            } else {
                let old_candle = std::mem::replace(
                    builder,
                    CandleBuilder::new(trade, interval_start)
                ).to_candle();

                self.completed_candles.insert(old_candle.timestamp, old_candle.clone());
                completed_candle = Some(old_candle);

                if self.completed_candles.len() > self.max_candles {
                    if let Some(oldest_key) = self.completed_candles.keys().next().copied() {
                        self.completed_candles.remove(&oldest_key);
                    }
                }
            }
        } else {
            self.current_builder = Some(CandleBuilder::new(trade, interval_start));
        }

        completed_candle
    }

    pub fn get_current_candle(&self) -> Option<Candle> {
        self.current_builder.as_ref().map(|builder| Candle {
            timestamp: builder.timestamp,
            open: builder.open,
            high: builder.high,
            low: builder.low,
            close: builder.close,
            volume: builder.volume,
        })
    }

    pub fn get_candles(&self) -> Vec<Candle> {
        let mut candles: Vec<Candle> = self.completed_candles.values().cloned().collect();

        if let Some(current) = self.get_current_candle() {
            candles.push(current);
        }

        candles
    }

    pub fn clear(&mut self) {
        self.current_builder = None;
        self.completed_candles.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;
    use trade_shared::Side;

    #[test]
    fn test_interval_seconds() {
        assert_eq!(AggregationInterval::Sec1.seconds(), 1);
        assert_eq!(AggregationInterval::Sec5.seconds(), 5);
        assert_eq!(AggregationInterval::Sec10.seconds(), 10);
        assert_eq!(AggregationInterval::Sec30.seconds(), 30);
    }

    #[test]
    fn test_interval_string_conversion() {
        assert_eq!(AggregationInterval::Sec1.to_interval_string(), "1s");
        assert_eq!(AggregationInterval::from_interval_string("1s"), Some(AggregationInterval::Sec1));
        assert_eq!(AggregationInterval::from_interval_string("invalid"), None);
    }

    #[test]
    fn test_get_interval_start() {
        let aggregator = TickAggregator::new(AggregationInterval::Sec5);

        assert_eq!(aggregator.get_interval_start(1000), 0);
        assert_eq!(aggregator.get_interval_start(5000), 5000);
        assert_eq!(aggregator.get_interval_start(7500), 5000);
        assert_eq!(aggregator.get_interval_start(10000), 10000);
    }

    #[test]
    fn test_single_trade_aggregation() {
        let mut aggregator = TickAggregator::new(AggregationInterval::Sec1);

        let trade = Trade {
            timestamp: 1000,
            price: dec!(100.0),
            qty: dec!(1.0),
            side: Side::Buy,
        };

        let result = aggregator.add_trade(&trade);
        assert!(result.is_none());

        let current = aggregator.get_current_candle().unwrap();
        assert_eq!(current.timestamp, 1000);
        assert_eq!(current.open, dec!(100.0));
        assert_eq!(current.high, dec!(100.0));
        assert_eq!(current.low, dec!(100.0));
        assert_eq!(current.close, dec!(100.0));
        assert_eq!(current.volume, dec!(1.0));
    }

    #[test]
    fn test_multiple_trades_same_interval() {
        let mut aggregator = TickAggregator::new(AggregationInterval::Sec1);

        let trades = vec![
            Trade { timestamp: 1000, price: dec!(100.0), qty: dec!(1.0), side: Side::Buy },
            Trade { timestamp: 1500, price: dec!(105.0), qty: dec!(2.0), side: Side::Buy },
            Trade { timestamp: 1800, price: dec!(95.0), qty: dec!(1.5), side: Side::Sell },
        ];

        for trade in &trades {
            aggregator.add_trade(trade);
        }

        let current = aggregator.get_current_candle().unwrap();
        assert_eq!(current.timestamp, 1000);
        assert_eq!(current.open, dec!(100.0));
        assert_eq!(current.high, dec!(105.0));
        assert_eq!(current.low, dec!(95.0));
        assert_eq!(current.close, dec!(95.0));
        assert_eq!(current.volume, dec!(4.5));
    }

    #[test]
    fn test_candle_completion() {
        let mut aggregator = TickAggregator::new(AggregationInterval::Sec1);

        let trade1 = Trade { timestamp: 1000, price: dec!(100.0), qty: dec!(1.0), side: Side::Buy };
        let trade2 = Trade { timestamp: 2000, price: dec!(105.0), qty: dec!(2.0), side: Side::Buy };

        let result1 = aggregator.add_trade(&trade1);
        assert!(result1.is_none());

        let result2 = aggregator.add_trade(&trade2);
        assert!(result2.is_some());

        let completed = result2.unwrap();
        assert_eq!(completed.timestamp, 1000);
        assert_eq!(completed.close, dec!(100.0));

        let current = aggregator.get_current_candle().unwrap();
        assert_eq!(current.timestamp, 2000);
        assert_eq!(current.open, dec!(105.0));
    }

    #[test]
    fn test_5_second_interval() {
        let mut aggregator = TickAggregator::new(AggregationInterval::Sec5);

        let trades = vec![
            Trade { timestamp: 1000, price: dec!(100.0), qty: dec!(1.0), side: Side::Buy },
            Trade { timestamp: 3000, price: dec!(105.0), qty: dec!(1.0), side: Side::Buy },
            Trade { timestamp: 5000, price: dec!(110.0), qty: dec!(1.0), side: Side::Buy },
        ];

        aggregator.add_trade(&trades[0]);
        aggregator.add_trade(&trades[1]);

        let current = aggregator.get_current_candle().unwrap();
        assert_eq!(current.timestamp, 0);
        assert_eq!(current.volume, dec!(2.0));

        let completed = aggregator.add_trade(&trades[2]);
        assert!(completed.is_some());

        let new_current = aggregator.get_current_candle().unwrap();
        assert_eq!(new_current.timestamp, 5000);
        assert_eq!(new_current.open, dec!(110.0));
    }
}
