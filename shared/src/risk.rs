use rust_decimal::Decimal;
use rust_decimal_macros::dec;

use crate::Side;

pub const TAKER_FEE: Decimal = dec!(0.00055);
pub const MAKER_FEE: Decimal = dec!(0.0002);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RiskError {
    InvalidRisk,
    InvalidSlPrice,
    InvalidEntryPrice,
    SlInvalidForSide,
    RiskPerUnitTooSmall,
}

#[derive(Debug)]
pub struct RiskCalculation {
    pub quantity: Decimal,
    pub entry_price: Decimal,
    pub sl_price: Decimal,
    pub risk_per_unit: Decimal,
    pub fee_cost: Decimal,
}

pub fn calculate_position_size(
    risk_usdt: Decimal,
    entry_price: Decimal,
    sl_price: Decimal,
    side: Side,
    is_limit: bool,
) -> Result<RiskCalculation, RiskError> {
    if risk_usdt <= Decimal::ZERO {
        return Err(RiskError::InvalidRisk);
    }

    if entry_price <= Decimal::ZERO {
        return Err(RiskError::InvalidEntryPrice);
    }

    if sl_price <= Decimal::ZERO {
        return Err(RiskError::InvalidSlPrice);
    }

    let valid_sl = match side {
        Side::Buy => sl_price < entry_price,
        Side::Sell => sl_price > entry_price,
    };

    if !valid_sl {
        return Err(RiskError::SlInvalidForSide);
    }

    let fee = if is_limit { MAKER_FEE } else { TAKER_FEE };
    let price_diff = (entry_price - sl_price).abs();
    let fee_cost = entry_price * fee * dec!(2);
    let risk_per_unit = price_diff + fee_cost;

    if risk_per_unit <= Decimal::ZERO {
        return Err(RiskError::RiskPerUnitTooSmall);
    }

    let quantity = risk_usdt / risk_per_unit;

    Ok(RiskCalculation {
        quantity,
        entry_price,
        sl_price,
        risk_per_unit,
        fee_cost,
    })
}

pub fn round_quantity(quantity: Decimal, decimals: u32) -> Decimal {
    quantity.round_dp(decimals)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_long_market_order() {
        let result = calculate_position_size(
            dec!(10),      // risk $10
            dec!(95000),   // entry
            dec!(94000),   // sl
            Side::Buy,
            false,         // market order
        ).unwrap();

        // price_diff = 1000
        // fee_cost = 95000 * 0.00055 * 2 = 104.5
        // risk_per_unit = 1104.5
        // qty = 10 / 1104.5 = 0.00905...

        assert_eq!(result.entry_price, dec!(95000));
        assert_eq!(result.sl_price, dec!(94000));
        assert_eq!(result.fee_cost, dec!(104.5));
        assert_eq!(result.risk_per_unit, dec!(1104.5));
        
        let qty_rounded = round_quantity(result.quantity, 6);
        assert_eq!(qty_rounded, dec!(0.009054));
    }

    #[test]
    fn test_long_limit_order() {
        let result = calculate_position_size(
            dec!(10),
            dec!(95000),
            dec!(94000),
            Side::Buy,
            true,          // limit order (maker fee)
        ).unwrap();

        // fee_cost = 95000 * 0.0002 * 2 = 38
        // risk_per_unit = 1000 + 38 = 1038
        // qty = 10 / 1038 = 0.00963...

        assert_eq!(result.fee_cost, dec!(38));
        assert_eq!(result.risk_per_unit, dec!(1038));
        
        let qty_rounded = round_quantity(result.quantity, 6);
        assert_eq!(qty_rounded, dec!(0.009634));
    }

    #[test]
    fn test_short_market_order() {
        let result = calculate_position_size(
            dec!(10),
            dec!(95000),   // entry
            dec!(96000),   // sl above entry for short
            Side::Sell,
            false,
        ).unwrap();

        assert_eq!(result.entry_price, dec!(95000));
        assert_eq!(result.sl_price, dec!(96000));
        
        // price_diff = 1000
        // fee_cost = 104.5
        // risk_per_unit = 1104.5
        assert_eq!(result.risk_per_unit, dec!(1104.5));
    }

    #[test]
    fn test_short_limit_order() {
        let result = calculate_position_size(
            dec!(20),      // risk $20
            dec!(95000),
            dec!(95500),   // sl 500 above
            Side::Sell,
            true,
        ).unwrap();

        // price_diff = 500
        // fee_cost = 95000 * 0.0002 * 2 = 38
        // risk_per_unit = 538
        // qty = 20 / 538 = 0.03717...

        assert_eq!(result.risk_per_unit, dec!(538));
        
        let qty_rounded = round_quantity(result.quantity, 6);
        assert_eq!(qty_rounded, dec!(0.037175));
    }

    #[test]
    fn test_invalid_sl_for_long() {
        let result = calculate_position_size(
            dec!(10),
            dec!(95000),
            dec!(96000),   // sl above entry - invalid for long
            Side::Buy,
            false,
        );

        assert_eq!(result.unwrap_err(), RiskError::SlInvalidForSide);
    }

    #[test]
    fn test_invalid_sl_for_short() {
        let result = calculate_position_size(
            dec!(10),
            dec!(95000),
            dec!(94000),   // sl below entry - invalid for short
            Side::Sell,
            false,
        );

        assert_eq!(result.unwrap_err(), RiskError::SlInvalidForSide);
    }

    #[test]
    fn test_zero_risk() {
        let result = calculate_position_size(
            dec!(0),
            dec!(95000),
            dec!(94000),
            Side::Buy,
            false,
        );

        assert_eq!(result.unwrap_err(), RiskError::InvalidRisk);
    }

    #[test]
    fn test_negative_risk() {
        let result = calculate_position_size(
            dec!(-10),
            dec!(95000),
            dec!(94000),
            Side::Buy,
            false,
        );

        assert_eq!(result.unwrap_err(), RiskError::InvalidRisk);
    }

    #[test]
    fn test_zero_entry_price() {
        let result = calculate_position_size(
            dec!(10),
            dec!(0),
            dec!(94000),
            Side::Buy,
            false,
        );

        assert_eq!(result.unwrap_err(), RiskError::InvalidEntryPrice);
    }

    #[test]
    fn test_zero_sl_price() {
        let result = calculate_position_size(
            dec!(10),
            dec!(95000),
            dec!(0),
            Side::Buy,
            false,
        );

        assert_eq!(result.unwrap_err(), RiskError::InvalidSlPrice);
    }

    #[test]
    fn test_small_risk_small_sl_distance() {
        // Very tight SL
        let result = calculate_position_size(
            dec!(1),       // risk $1
            dec!(95000),
            dec!(94990),   // only $10 away
            Side::Buy,
            false,
        ).unwrap();

        // price_diff = 10
        // fee_cost = 104.5
        // risk_per_unit = 114.5
        // qty = 1 / 114.5 = 0.00873...

        assert_eq!(result.risk_per_unit, dec!(114.5));
        
        let qty_rounded = round_quantity(result.quantity, 6);
        assert_eq!(qty_rounded, dec!(0.008734));
    }

    #[test]
    fn test_large_risk_large_sl_distance() {
        let result = calculate_position_size(
            dec!(1000),    // risk $1000
            dec!(95000),
            dec!(90000),   // $5000 away
            Side::Buy,
            false,
        ).unwrap();

        // price_diff = 5000
        // fee_cost = 104.5
        // risk_per_unit = 5104.5
        // qty = 1000 / 5104.5 = 0.1959...

        assert_eq!(result.risk_per_unit, dec!(5104.5));
        
        let qty_rounded = round_quantity(result.quantity, 4);
        assert_eq!(qty_rounded, dec!(0.1959));
    }

    #[test]
    fn test_eth_pair() {
        // Test with different price scale (ETH ~3000)
        let result = calculate_position_size(
            dec!(50),      // risk $50
            dec!(3000),
            dec!(2900),    // $100 away
            Side::Buy,
            false,
        ).unwrap();

        // price_diff = 100
        // fee_cost = 3000 * 0.00055 * 2 = 3.3
        // risk_per_unit = 103.3
        // qty = 50 / 103.3 = 0.484...

        assert_eq!(result.fee_cost, dec!(3.3));
        assert_eq!(result.risk_per_unit, dec!(103.3));
        
        let qty_rounded = round_quantity(result.quantity, 4);
        assert_eq!(qty_rounded, dec!(0.4840));
    }

    #[test]
    fn test_sl_equals_entry() {
        let result = calculate_position_size(
            dec!(10),
            dec!(95000),
            dec!(95000),   // sl = entry
            Side::Buy,
            false,
        );

        assert_eq!(result.unwrap_err(), RiskError::SlInvalidForSide);
    }
}
