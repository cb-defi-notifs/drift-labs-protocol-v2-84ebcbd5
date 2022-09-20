use solana_program::native_token::LAMPORTS_PER_SOL;
pub const LAMPORTS_PER_SOL_I128: i128 = LAMPORTS_PER_SOL as i128;

// PRECISIONS
pub const AMM_RESERVE_PRECISION: u128 = 10_000_000_000_000; //expo = -13;
pub const BASE_PRECISION: u128 = AMM_RESERVE_PRECISION; //expo = -13;
pub const MARK_PRICE_PRECISION: u128 = 10_000_000_000; //expo = -10;
pub const MARK_PRICE_PRECISION_I128: i128 = MARK_PRICE_PRECISION as i128;
pub const QUOTE_PRECISION: u128 = 1_000_000; // expo = -6
pub const QUOTE_PRECISION_I128: i128 = 1_000_000; // expo = -6
pub const QUOTE_PRECISION_I64: i64 = 1_000_000; // expo = -6
pub const QUOTE_PRECISION_U64: u64 = 1_000_000; // expo = -6
pub const FUNDING_PAYMENT_PRECISION: u128 = 10_000; // expo = -4
pub const FUNDING_PAYMENT_PRECISION_I128: i128 = 10_000; // expo = -4
pub const MARGIN_PRECISION: u128 = 10_000; // expo = -4
pub const PEG_PRECISION: u128 = 1_000; //expo = -3
pub const BID_ASK_SPREAD_PRECISION: u128 = 1_000_000; // expo = -6
pub const BID_ASK_SPREAD_PRECISION_I128: i128 = (BID_ASK_SPREAD_PRECISION) as i128;
pub const LAMPORT_PER_SOL: u128 = LAMPORTS_PER_SOL as u128;
pub const LAMPORT_PER_SOL_I128: i128 = LAMPORTS_PER_SOL as i128;
pub const PERP_DECIMALS: u32 = 13;
pub const CONCENTRATION_PRECISION: u128 = 1_000_000;

pub const AMM_RESERVE_PRECISION_I128: i128 = (AMM_RESERVE_PRECISION) as i128;
pub const BASE_PRECISION_I128: i128 = AMM_RESERVE_PRECISION_I128;
pub const SPOT_INTEREST_PRECISION: u128 = 1_000_000; // expo = -6
pub const SPOT_CUMULATIVE_INTEREST_PRECISION: u128 = 10_000_000_000; // expo = -10
pub const SPOT_UTILIZATION_PRECISION: u128 = 1_000_000; // expo = -6
pub const LIQUIDATION_FEE_PRECISION: u128 = 1_000_000; // expo = -6
pub const SPOT_WEIGHT_PRECISION: u128 = MARGIN_PRECISION; // expo = -4
pub const SPOT_IMF_PRECISION: u128 = 1_000_000; // expo = -6

// PRECISION CONVERSIONS
pub const PRICE_TO_PEG_PRECISION_RATIO: u128 = MARK_PRICE_PRECISION / PEG_PRECISION; // expo: 7
pub const PRICE_TO_PEG_QUOTE_PRECISION_RATIO: u128 = MARK_PRICE_PRECISION / QUOTE_PRECISION; // expo: 4
pub const AMM_TO_QUOTE_PRECISION_RATIO: u128 = AMM_RESERVE_PRECISION / QUOTE_PRECISION; // expo: 7
pub const AMM_TO_QUOTE_PRECISION_RATIO_I128: i128 =
    (AMM_RESERVE_PRECISION / QUOTE_PRECISION) as i128; // expo: 7
pub const AMM_TIMES_PEG_TO_QUOTE_PRECISION_RATIO: u128 =
    AMM_RESERVE_PRECISION * PEG_PRECISION / QUOTE_PRECISION; // expo: 10
pub const QUOTE_TO_BASE_AMT_FUNDING_PRECISION: i128 =
    (AMM_RESERVE_PRECISION * MARK_PRICE_PRECISION * FUNDING_PAYMENT_PRECISION / QUOTE_PRECISION)
        as i128; // expo: 21
pub const PRICE_TO_QUOTE_PRECISION_RATIO: u128 = MARK_PRICE_PRECISION / QUOTE_PRECISION; // expo: 4
pub const MARK_PRICE_TIMES_AMM_TO_QUOTE_PRECISION_RATIO: u128 =
    MARK_PRICE_PRECISION * AMM_TO_QUOTE_PRECISION_RATIO; // expo 17
pub const LIQUIDATION_FEE_TO_MARGIN_PRECISION_RATIO: u128 =
    LIQUIDATION_FEE_PRECISION / MARGIN_PRECISION;
pub const LIQUIDATION_FEE_TO_SPOT_WEIGHT_PRECISION_RATIO: u128 =
    LIQUIDATION_FEE_PRECISION / SPOT_WEIGHT_PRECISION;
pub const MARGIN_PRECISION_TO_SPOT_WEIGHT_PRECISION_RATIO: u128 =
    MARGIN_PRECISION / SPOT_WEIGHT_PRECISION;
pub const FUNDING_RATE_TO_QUOTE_PRECISION_PRECISION_RATIO: u128 =
    MARK_PRICE_PRECISION * FUNDING_PAYMENT_PRECISION / QUOTE_PRECISION;
pub const FUNDING_RATE_PRECISION_I128: i128 =
    MARK_PRICE_PRECISION_I128 * FUNDING_PAYMENT_PRECISION_I128; // expo: 14

pub const AMM_TIMES_PEG_TO_QUOTE_PRECISION_RATIO_I128: i128 =
    AMM_TIMES_PEG_TO_QUOTE_PRECISION_RATIO as i128;
pub const MARK_PRICE_TIMES_AMM_TO_QUOTE_PRECISION_RATIO_I128: i128 =
    MARK_PRICE_TIMES_AMM_TO_QUOTE_PRECISION_RATIO as i128; // expo 17

pub const FUNDING_EXCESS_TO_QUOTE_RATIO: i128 =
    (MARK_PRICE_PRECISION * AMM_RESERVE_PRECISION / QUOTE_PRECISION) as i128; // expo 17

pub const AMM_TIMES_PEG_PRECISION: i128 = (AMM_RESERVE_PRECISION * PEG_PRECISION) as i128; // expo 16

// FEE REBATES
pub const SHARE_OF_FEES_ALLOCATED_TO_CLEARING_HOUSE_NUMERATOR: u128 = 1;
pub const SHARE_OF_FEES_ALLOCATED_TO_CLEARING_HOUSE_DENOMINATOR: u128 = 2;

pub const SHARE_OF_IF_ESCROW_ALLOCATED_TO_PROTOCOL_NUMERATOR: u128 = 1;
pub const SHARE_OF_IF_ESCROW_ALLOCATED_TO_PROTOCOL_DENOMINATOR: u128 = 2;

pub const SHARE_OF_REVENUE_ALLOCATED_TO_INSURANCE_FUND_VAULT_NUMERATOR: u128 = 1; // todo
pub const SHARE_OF_REVENUE_ALLOCATED_TO_INSURANCE_FUND_VAULT_DENOMINATOR: u128 = 1;

pub const MAX_APR_PER_REVENUE_SETTLE_TO_INSURANCE_FUND_VAULT: u64 = 1000; // 1000% APR
pub const MAX_APR_PER_REVENUE_SETTLE_PRECISION: u64 = 10; // 1000% APR -> 10

pub const UPDATE_K_ALLOWED_PRICE_CHANGE: u128 = MARK_PRICE_PRECISION / 10_000; //.0001

// TIME PERIODS
// pub const ONE_HOUR: i64 = 3600;
pub const ONE_HOUR: i128 = 3600;
pub const ONE_HOUR_I128: i128 = ONE_HOUR as i128;
pub const TWENTY_FOUR_HOUR: i64 = 3600 * 24;
pub const THIRTY_DAY_I128: i128 = (TWENTY_FOUR_HOUR * 30) as i128;
pub const ONE_YEAR: u128 = 31536000;
pub const EPOCH_DURATION: i64 = TWENTY_FOUR_HOUR * 28;

// FEES
pub const ONE_BPS_DENOMINATOR: u32 = 10000;
pub const ONE_HUNDRED_MILLION_QUOTE: u64 = 100_000_000_u64 * QUOTE_PRECISION_U64;
pub const TEN_MILLION_QUOTE: u64 = 10_000_000_u64 * QUOTE_PRECISION_U64;
pub const ONE_MILLION_QUOTE: u64 = 1_000_000_u64 * QUOTE_PRECISION_U64;
pub const MAX_REFERRER_REWARD_EPOCH_UPPER_BOUND: u64 = (4000 * QUOTE_PRECISION) as u64;
pub const LP_FEE_SLICE_NUMERATOR: u128 = 8;
pub const LP_FEE_SLICE_DENOMINATOR: u128 = 10;

// CONSTRAINTS
pub const MAX_CONCENTRATION_COEFFICIENT: u128 = 1_414_200;
pub const MAX_LIQUIDATION_SLIPPAGE: i128 = 10_000; // expo = -2
pub const MAX_LIQUIDATION_SLIPPAGE_U128: u128 = 10_000; // expo = -2
pub const MAX_MARK_TWAP_DIVERGENCE: u128 = 500_000; // expo = -3
pub const MAXIMUM_MARGIN_RATIO: u32 = MARGIN_PRECISION as u32;
pub const MINIMUM_MARGIN_RATIO: u32 = MARGIN_PRECISION as u32 / 50;
pub const MAX_BID_ASK_INVENTORY_SKEW_FACTOR: u128 = 5 * BID_ASK_SPREAD_PRECISION;

// FORMULAIC REPEG / K
pub const K_BPS_UPDATE_SCALE: i128 = 1_000_000; // expo = -6 (represents 100%)
                                                // hardcoded scale bounds for a single k update (.1% increase and .09% decrease). scaled by market curve_update_intensity
pub const K_BPS_DECREASE_MAX: i128 = 22000; // 2.2% decrease (25000/K_BPS_UPDATE_SCALE)
pub const K_BPS_INCREASE_MAX: i128 = 1000; // 10 bps increase

pub const PEG_BPS_UPDATE_SCALE: u128 = 1_000_000; // expo = -6 (represents 100%)
                                                  // hardcoded scale bounds for a single repeg update. scaled by market curve_update_intensity
pub const PEG_BPS_DECREASE_MAX: u128 = 1000; // 10 bps decrease
pub const PEG_BPS_INCREASE_MAX: u128 = 1000; // 10 bps increase

pub const QUOTE_SPOT_MARKET_INDEX: u64 = 0;
