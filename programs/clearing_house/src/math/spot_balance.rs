use solana_program::msg;

use crate::error::{ClearingHouseResult, ErrorCode};
use crate::math::casting::Cast;
use crate::math::constants::{ONE_YEAR, SPOT_RATE_PRECISION, SPOT_UTILIZATION_PRECISION};
use crate::math::safe_math::SafeMath;
use crate::state::oracle::OraclePriceData;
use crate::state::spot_market::{SpotBalanceType, SpotMarket};
use crate::state::user::SpotPosition;
use crate::validate;

pub fn get_spot_balance(
    token_amount: u128,
    spot_market: &SpotMarket,
    balance_type: &SpotBalanceType,
    round_up: bool,
) -> ClearingHouseResult<u128> {
    let precision_increase = 10_u128.pow(19_u8.safe_sub(spot_market.decimals)?.into());

    let cumulative_interest = match balance_type {
        SpotBalanceType::Deposit => spot_market.cumulative_deposit_interest,
        SpotBalanceType::Borrow => spot_market.cumulative_borrow_interest,
    };

    let mut balance = token_amount
        .safe_mul(precision_increase)?
        .safe_div(cumulative_interest)?;

    if round_up && balance != 0 {
        balance = balance.safe_add(1)?;
    }

    Ok(balance)
}

pub fn get_token_amount(
    balance: u128,
    spot_market: &SpotMarket,
    balance_type: &SpotBalanceType,
) -> ClearingHouseResult<u128> {
    let precision_decrease = 10_u128.pow(19_u8.safe_sub(spot_market.decimals)?.into());

    let cumulative_interest = match balance_type {
        SpotBalanceType::Deposit => spot_market.cumulative_deposit_interest,
        SpotBalanceType::Borrow => spot_market.cumulative_borrow_interest,
    };

    let token_amount = balance
        .safe_mul(cumulative_interest)?
        .safe_div(precision_decrease)?;

    Ok(token_amount)
}

pub fn get_signed_token_amount(
    token_amount: u128,
    balance_type: &SpotBalanceType,
) -> ClearingHouseResult<i128> {
    match balance_type {
        SpotBalanceType::Deposit => token_amount.cast(),
        SpotBalanceType::Borrow => token_amount
            .cast::<i128>()
            .map(|token_amount| -token_amount),
    }
}

pub fn get_interest_token_amount(
    balance: u128,
    spot_market: &SpotMarket,
    interest: u128,
) -> ClearingHouseResult<u128> {
    let precision_decrease = 10_u128.pow(19_u8.safe_sub(spot_market.decimals)?.into());

    let token_amount = balance.safe_mul(interest)?.safe_div(precision_decrease)?;

    Ok(token_amount)
}

pub struct InterestAccumulated {
    pub borrow_interest: u128,
    pub deposit_interest: u128,
}

pub fn calculate_utilization(
    deposit_token_amount: u128,
    borrow_token_amount: u128,
) -> ClearingHouseResult<u128> {
    let utilization = borrow_token_amount
        .safe_mul(SPOT_UTILIZATION_PRECISION)?
        .checked_div(deposit_token_amount)
        .or({
            if deposit_token_amount == 0 && borrow_token_amount == 0 {
                Some(0_u128)
            } else {
                // if there are borrows without deposits, default to maximum utilization rate
                Some(SPOT_UTILIZATION_PRECISION)
            }
        })
        .unwrap();

    Ok(utilization)
}

pub fn calculate_accumulated_interest(
    spot_market: &SpotMarket,
    now: i64,
) -> ClearingHouseResult<InterestAccumulated> {
    let deposit_token_amount = get_token_amount(
        spot_market.deposit_balance,
        spot_market,
        &SpotBalanceType::Deposit,
    )?;
    let borrow_token_amount = get_token_amount(
        spot_market.borrow_balance,
        spot_market,
        &SpotBalanceType::Borrow,
    )?;

    let utilization = calculate_utilization(deposit_token_amount, borrow_token_amount)?;

    if utilization == 0 {
        return Ok(InterestAccumulated {
            borrow_interest: 0,
            deposit_interest: 0,
        });
    }

    let borrow_rate = if utilization > spot_market.optimal_utilization.cast()? {
        let surplus_utilization = utilization.safe_sub(spot_market.optimal_utilization.cast()?)?;

        let borrow_rate_slope = spot_market
            .max_borrow_rate
            .cast::<u128>()?
            .safe_sub(spot_market.optimal_borrow_rate.cast()?)?
            .safe_mul(SPOT_UTILIZATION_PRECISION)?
            .safe_div(
                SPOT_UTILIZATION_PRECISION.safe_sub(spot_market.optimal_utilization.cast()?)?,
            )?;

        spot_market.optimal_borrow_rate.cast::<u128>()?.safe_add(
            surplus_utilization
                .safe_mul(borrow_rate_slope)?
                .safe_div(SPOT_UTILIZATION_PRECISION)?,
        )?
    } else {
        let borrow_rate_slope = spot_market
            .optimal_borrow_rate
            .cast::<u128>()?
            .safe_mul(SPOT_UTILIZATION_PRECISION)?
            .safe_div(spot_market.optimal_utilization.cast()?)?;

        utilization
            .safe_mul(borrow_rate_slope)?
            .safe_div(SPOT_UTILIZATION_PRECISION)?
    };

    let time_since_last_update = now
        .cast::<u64>()
        .or(Err(ErrorCode::UnableToCastUnixTime))?
        .safe_sub(spot_market.last_interest_ts)?;

    // To save some compute units, have to multiply the rate by the `time_since_last_update` here
    // and then divide out by ONE_YEAR when calculating interest accumulated below
    let modified_borrow_rate = borrow_rate.safe_mul(time_since_last_update as u128)?;

    let modified_deposit_rate = modified_borrow_rate
        .safe_mul(utilization)?
        .safe_div(SPOT_UTILIZATION_PRECISION)?;

    let borrow_interest = spot_market
        .cumulative_borrow_interest
        .safe_mul(modified_borrow_rate)?
        .safe_div(ONE_YEAR)?
        .safe_div(SPOT_RATE_PRECISION)?
        .safe_add(1)?;

    let deposit_interest = spot_market
        .cumulative_deposit_interest
        .safe_mul(modified_deposit_rate)?
        .safe_div(ONE_YEAR)?
        .safe_div(SPOT_RATE_PRECISION)?;

    Ok(InterestAccumulated {
        borrow_interest,
        deposit_interest,
    })
}

pub fn get_balance_value_and_token_amount(
    spot_position: &SpotPosition,
    spot_market: &SpotMarket,
    oracle_price_data: &OraclePriceData,
) -> ClearingHouseResult<(u128, u128)> {
    let token_amount = spot_position.get_token_amount(spot_market)?;

    let precision_decrease = 10_u128.pow(spot_market.decimals as u32);

    let value = token_amount
        .safe_mul(oracle_price_data.price.cast()?)?
        .safe_div(precision_decrease)?;

    Ok((value, token_amount))
}

pub fn get_strict_token_value(
    token_amount: i128,
    spot_decimals: u8,
    oracle_price_data: &OraclePriceData,
    oracle_price_twap: i64,
) -> ClearingHouseResult<i128> {
    if token_amount == 0 {
        return Ok(0);
    }

    let precision_decrease = 10_i128.pow(spot_decimals as u32);

    validate!(
        oracle_price_twap > 0,
        ErrorCode::InvalidOracle,
        "oracle_price_data={:?} oracle_price_twap={} (<= 0)",
        oracle_price_data,
        oracle_price_twap
    )?;

    let price = if token_amount > 0 {
        oracle_price_data.price.min(oracle_price_twap)
    } else {
        oracle_price_data.price.max(oracle_price_twap)
    };

    token_amount
        .safe_mul(price.cast()?)?
        .safe_div(precision_decrease)
}

pub fn get_token_value(
    token_amount: i128,
    spot_decimals: u8,
    oracle_price_data: &OraclePriceData,
) -> ClearingHouseResult<i128> {
    if token_amount == 0 {
        return Ok(0);
    }

    let precision_decrease = 10_i128.pow(spot_decimals as u32);

    token_amount
        .safe_mul(oracle_price_data.price.cast()?)?
        .safe_div(precision_decrease)
}

pub fn get_balance_value(
    spot_position: &SpotPosition,
    spot_market: &SpotMarket,
    oracle_price_data: &OraclePriceData,
) -> ClearingHouseResult<u128> {
    let (value, _) =
        get_balance_value_and_token_amount(spot_position, spot_market, oracle_price_data)?;
    Ok(value)
}
