use crate::controller::bank_balance::{update_bank_balances, update_bank_cumulative_interest};
use crate::controller::funding::settle_funding_payment;
use crate::controller::orders::cancel_order;
use crate::controller::position::{
    get_position_index, update_position_and_market, update_unsettled_pnl,
};
use crate::error::{ClearingHouseResult, ErrorCode};
use crate::math::bank_balance::get_token_amount;
use crate::math::casting::{cast, cast_to_i128};
use crate::math::constants::{BANK_WEIGHT_PRECISION, LIQUIDATION_FEE_PRECISION, MARGIN_PRECISION};
use crate::math::liquidation::{
    calculate_asset_transfer_for_liability_transfer,
    calculate_base_asset_amount_to_cover_margin_shortage,
    calculate_liability_transfer_implied_by_asset_amount,
    calculate_liability_transfer_to_cover_margin_shortage, get_margin_requirement_plus_buffer,
};
use crate::math::margin::{
    calculate_margin_requirement_and_total_collateral, meets_initial_margin_requirement,
    MarginRequirementType,
};
use crate::math::orders::{get_position_delta_for_fill, standardize_base_asset_amount_ceil};
use crate::math::position::calculate_base_asset_value_with_oracle_price;
use crate::math_error;
use crate::state::bank::BankBalanceType;
use crate::state::bank_map::BankMap;
use crate::state::events::OrderActionExplanation;
use crate::state::market_map::MarketMap;
use crate::state::oracle_map::OracleMap;
use crate::state::user::User;
use crate::validate;
use anchor_lang::prelude::Pubkey;
use solana_program::msg;
use std::ops::DerefMut;

#[cfg(test)]
mod tests;

pub fn liquidate_perp(
    market_index: u64,
    liquidator_max_base_asset_amount: u128,
    user: &mut User,
    user_key: &Pubkey,
    liquidator: &mut User,
    liquidator_key: &Pubkey,
    market_map: &MarketMap,
    bank_map: &BankMap,
    oracle_map: &mut OracleMap,
    slot: u64,
    now: i64,
    liquidation_margin_buffer_ratio: u8,
) -> ClearingHouseResult {
    user.get_position(market_index).map_err(|e| {
        msg!(
            "User does not have a position for perp market {}",
            market_index
        );
        e
    })?;

    liquidator
        .force_get_position_mut(market_index)
        .map_err(|e| {
            msg!("Liquidator has no available positions to take on perp position");
            e
        })?;

    // Settle user's funding payments so that collateral is up to date
    settle_funding_payment(
        user,
        user_key,
        market_map.get_ref_mut(&market_index)?.deref_mut(),
        now,
    )?;

    // Settle user's funding payments so that collateral is up to date
    settle_funding_payment(
        liquidator,
        liquidator_key,
        market_map.get_ref_mut(&market_index)?.deref_mut(),
        now,
    )?;

    let (mut margin_requirement, total_collateral) =
        calculate_margin_requirement_and_total_collateral(
            user,
            market_map,
            MarginRequirementType::Maintenance,
            bank_map,
            oracle_map,
        )?;

    let mut margin_requirement_plus_buffer =
        get_margin_requirement_plus_buffer(margin_requirement, liquidation_margin_buffer_ratio)?;

    if !user.being_liquidated && total_collateral >= cast(margin_requirement)? {
        return Err(ErrorCode::SufficientCollateral);
    } else if user.being_liquidated && total_collateral >= cast(margin_requirement_plus_buffer)? {
        user.being_liquidated = false;
        return Ok(());
    }

    let position_index = get_position_index(&user.positions, market_index)?;
    validate!(
        user.positions[position_index].is_open_position()
            || user.positions[position_index].has_open_order(),
        ErrorCode::PositionDoesntHaveOpenPositionOrOrders
    )?;

    let worst_case_base_asset_amount_before =
        user.positions[position_index].worst_case_base_asset_amount()?;
    for order_index in 0..user.orders.len() {
        if !user.orders[order_index].is_open_order_for_market(market_index) {
            continue;
        }

        cancel_order(
            order_index,
            user,
            user_key,
            market_map,
            oracle_map,
            now,
            slot,
            OrderActionExplanation::CanceledForLiquidation,
            Some(liquidator_key),
            0,
            true,
        )?;
    }

    let worst_case_base_asset_amount_after =
        user.positions[position_index].worst_case_base_asset_amount()?;
    let worse_case_base_asset_amount_delta = worst_case_base_asset_amount_before
        .checked_sub(worst_case_base_asset_amount_after)
        .ok_or_else(math_error!())?;

    let (margin_ratio, oracle_price) = {
        let market = &mut market_map.get_ref(&market_index)?;
        let oracle_price = oracle_map.get_price_data(&market.amm.oracle)?.price;
        let margin_ratio = market.get_margin_ratio(
            worst_case_base_asset_amount_before.unsigned_abs(),
            MarginRequirementType::Maintenance,
        )?;

        (margin_ratio, oracle_price)
    };

    if worse_case_base_asset_amount_delta != 0 {
        let base_asset_value = calculate_base_asset_value_with_oracle_price(
            worse_case_base_asset_amount_delta,
            oracle_price,
        )?;

        let margin_requirement_delta = base_asset_value
            .checked_mul(margin_ratio as u128)
            .ok_or_else(math_error!())?
            .checked_div(MARGIN_PRECISION)
            .ok_or_else(math_error!())?;

        margin_requirement_plus_buffer = margin_requirement_plus_buffer
            .checked_sub(margin_requirement_delta)
            .ok_or_else(math_error!())?;

        margin_requirement = margin_requirement
            .checked_sub(margin_requirement_delta)
            .ok_or_else(math_error!())?;
    }

    if total_collateral >= cast(margin_requirement)? {
        user.being_liquidated = false;
        return Ok(());
    }

    if user.positions[position_index].base_asset_amount == 0 {
        msg!("User has no base asset amount");
        return Ok(());
    }

    validate!(
        liquidator_max_base_asset_amount != 0,
        ErrorCode::InvalidBaseAssetAmountForLiquidatePerp,
        "liquidator_max_base_asset_amount cant be 0"
    )?;

    let liquidator_max_base_asset_amount = standardize_base_asset_amount_ceil(
        liquidator_max_base_asset_amount,
        market_map
            .get_ref(&market_index)?
            .amm
            .base_asset_amount_step_size,
    )?;

    let user_base_asset_amount = user.positions[position_index]
        .base_asset_amount
        .unsigned_abs();

    let margin_shortage = cast_to_i128(margin_requirement_plus_buffer)?
        .checked_sub(total_collateral)
        .ok_or_else(math_error!())?
        .unsigned_abs();

    let liquidation_fee = market_map.get_ref(&market_index)?.liquidation_fee;
    let base_asset_amount_to_cover_margin_shortage =
        calculate_base_asset_amount_to_cover_margin_shortage(
            margin_shortage,
            margin_ratio,
            liquidation_fee,
            oracle_price,
        )?;

    let base_asset_amount = user_base_asset_amount
        .min(liquidator_max_base_asset_amount)
        .min(base_asset_amount_to_cover_margin_shortage);

    let quote_asset_amount = if user.positions[position_index].base_asset_amount > 0 {
        let liquidation_multiplier = LIQUIDATION_FEE_PRECISION
            .checked_sub(liquidation_fee)
            .ok_or_else(math_error!())?;
        let base_asset_value =
            calculate_base_asset_value_with_oracle_price(cast(base_asset_amount)?, oracle_price)?;
        base_asset_value
            .checked_mul(liquidation_multiplier)
            .ok_or_else(math_error!())?
            .checked_div(LIQUIDATION_FEE_PRECISION)
            .ok_or_else(math_error!())?
    } else {
        let liquidation_multiplier = LIQUIDATION_FEE_PRECISION
            .checked_add(liquidation_fee)
            .ok_or_else(math_error!())?;
        let base_asset_value =
            calculate_base_asset_value_with_oracle_price(cast(base_asset_amount)?, oracle_price)?;
        base_asset_value
            .checked_mul(liquidation_multiplier)
            .ok_or_else(math_error!())?
            .checked_div(LIQUIDATION_FEE_PRECISION)
            .ok_or_else(math_error!())?
    };

    let user_position_delta = get_position_delta_for_fill(
        base_asset_amount,
        quote_asset_amount,
        user.positions[position_index].get_direction_to_close(),
    )?;

    let liquidator_position_delta = get_position_delta_for_fill(
        base_asset_amount,
        quote_asset_amount,
        user.positions[position_index].get_direction(),
    )?;

    {
        let mut market = market_map.get_ref_mut(&market_index)?;

        let user_position = user.get_position_mut(market_index).unwrap();
        let user_pnl =
            update_position_and_market(user_position, &mut market, &user_position_delta)?;
        update_unsettled_pnl(user_position, &mut market, user_pnl)?;

        let liquidator_position = liquidator.force_get_position_mut(market_index).unwrap();
        let liquidator_pnl = update_position_and_market(
            liquidator_position,
            &mut market,
            &liquidator_position_delta,
        )?;
        update_unsettled_pnl(liquidator_position, &mut market, liquidator_pnl)?;
    }

    if base_asset_amount >= base_asset_amount_to_cover_margin_shortage {
        user.being_liquidated = false;
    }

    let liquidator_meets_initial_margin_requirement =
        meets_initial_margin_requirement(liquidator, market_map, bank_map, oracle_map)?;

    validate!(
        liquidator_meets_initial_margin_requirement,
        ErrorCode::InsufficientCollateral,
        "Liquidator doesnt have enough collateral to take over perp position"
    )?;

    Ok(())
}

pub fn liquidate_borrow(
    asset_bank_index: u64,
    liability_bank_index: u64,
    liquidator_max_liability_transfer: u128,
    user: &mut User,
    liquidator: &mut User,
    market_map: &MarketMap,
    bank_map: &BankMap,
    oracle_map: &mut OracleMap,
    now: i64,
    liquidation_margin_buffer_ratio: u8,
) -> ClearingHouseResult {
    // validate user and liquidator have bank balances
    user.get_bank_balance(asset_bank_index).ok_or_else(|| {
        msg!(
            "User does not have a bank balance for asset bank {}",
            asset_bank_index
        );
        ErrorCode::CouldNotFindBankBalance
    })?;

    user.get_bank_balance(liability_bank_index).ok_or_else(|| {
        msg!(
            "User does not have a bank balance for liability bank {}",
            liability_bank_index
        );
        ErrorCode::CouldNotFindBankBalance
    })?;

    match liquidator.get_bank_balance_mut(asset_bank_index) {
        Some(_) => {}
        None => {
            liquidator
                .add_bank_balance(asset_bank_index, BankBalanceType::Deposit)
                .map_err(|e| {
                    msg!("Liquidator has no available bank balances to take on deposit");
                    e
                })?;
        }
    };

    match liquidator.get_bank_balance_mut(liability_bank_index) {
        Some(_) => {}
        None => {
            liquidator
                .add_bank_balance(liability_bank_index, BankBalanceType::Borrow)
                .map_err(|e| {
                    msg!("Liquidator has no available bank balances to take on borrow");
                    e
                })?;
        }
    };

    let (asset_amount, asset_price, asset_decimals, asset_weight, asset_liquidation_multiplier) = {
        let mut asset_bank = bank_map.get_ref_mut(&asset_bank_index)?;
        update_bank_cumulative_interest(&mut asset_bank, now)?;

        let user_deposit_bank_balance = user.get_bank_balance(asset_bank_index).unwrap();

        validate!(
            user_deposit_bank_balance.balance_type == BankBalanceType::Deposit,
            ErrorCode::WrongBankBalanceType,
            "User did not have a deposit for the deposit bank index"
        )?;

        let token_amount = get_token_amount(
            user_deposit_bank_balance.balance,
            &asset_bank,
            &user_deposit_bank_balance.balance_type,
        )?;

        // TODO add oracle checks
        let token_price = oracle_map.get_price_data(&asset_bank.oracle)?.price;

        (
            token_amount,
            token_price,
            asset_bank.decimals,
            asset_bank.maintenance_asset_weight,
            LIQUIDATION_FEE_PRECISION
                .checked_add(asset_bank.liquidation_fee)
                .ok_or_else(math_error!())?,
        )
    };

    let (
        liability_amount,
        liability_price,
        liability_decimals,
        liability_weight,
        liability_liquidation_multiplier,
    ) = {
        let mut liability_bank = bank_map.get_ref_mut(&liability_bank_index)?;
        update_bank_cumulative_interest(&mut liability_bank, now)?;

        let user_bank_balance = user.get_bank_balance(liability_bank_index).unwrap();

        validate!(
            user_bank_balance.balance_type == BankBalanceType::Borrow,
            ErrorCode::WrongBankBalanceType,
            "User did not have a deposit for the borrow bank index"
        )?;

        let token_amount = get_token_amount(
            user_bank_balance.balance,
            &liability_bank,
            &user_bank_balance.balance_type,
        )?;

        // TODO add oracle checks
        let token_price = oracle_map.get_price_data(&liability_bank.oracle)?.price;

        (
            token_amount,
            token_price,
            liability_bank.decimals,
            // TODO should use size premium weight?
            liability_bank.maintenance_liability_weight,
            LIQUIDATION_FEE_PRECISION
                .checked_sub(liability_bank.liquidation_fee)
                .ok_or_else(math_error!())?,
        )
    };

    let (margin_requirement, total_collateral) = calculate_margin_requirement_and_total_collateral(
        user,
        market_map,
        MarginRequirementType::Maintenance,
        bank_map,
        oracle_map,
    )?;

    let margin_requirement_plus_buffer =
        get_margin_requirement_plus_buffer(margin_requirement, liquidation_margin_buffer_ratio)?;

    if !user.being_liquidated && total_collateral >= cast(margin_requirement)? {
        return Err(ErrorCode::SufficientCollateral);
    } else if user.being_liquidated && total_collateral >= cast(margin_requirement_plus_buffer)? {
        user.being_liquidated = false;
        return Ok(());
    }

    let margin_shortage = cast_to_i128(margin_requirement_plus_buffer)?
        .checked_sub(total_collateral)
        .ok_or_else(math_error!())?
        .unsigned_abs();

    // Determine what amount of borrow to transfer to reduce margin shortage to 0
    let liability_transfer_to_cover_margin_shortage =
        calculate_liability_transfer_to_cover_margin_shortage(
            margin_shortage,
            asset_weight,
            asset_liquidation_multiplier,
            liability_weight,
            liability_liquidation_multiplier,
            liability_decimals,
            liability_price,
        )?;

    // Given the user's deposit amount, how much borrow can be transferred?
    let liability_transfer_implied_by_asset_amount =
        calculate_liability_transfer_implied_by_asset_amount(
            asset_amount,
            asset_liquidation_multiplier,
            asset_decimals,
            asset_price,
            liability_liquidation_multiplier,
            liability_decimals,
            liability_price,
        )?;

    let liability_transfer = liquidator_max_liability_transfer
        .min(liability_amount)
        .min(liability_transfer_to_cover_margin_shortage)
        .min(liability_transfer_implied_by_asset_amount);

    // Given the borrow amount to transfer, determine how much deposit amount to transfer
    let asset_transfer = calculate_asset_transfer_for_liability_transfer(
        asset_amount,
        asset_liquidation_multiplier,
        asset_decimals,
        asset_price,
        liability_transfer,
        liability_liquidation_multiplier,
        liability_decimals,
        liability_price,
    )?;

    {
        let mut liability_bank = bank_map.get_ref_mut(&liability_bank_index)?;

        update_bank_balances(
            liability_transfer,
            &BankBalanceType::Deposit,
            &mut liability_bank,
            user.get_bank_balance_mut(liability_bank_index).unwrap(),
        )?;

        update_bank_balances(
            liability_transfer,
            &BankBalanceType::Borrow,
            &mut liability_bank,
            liquidator
                .get_bank_balance_mut(liability_bank_index)
                .unwrap(),
        )?;
    }

    {
        let mut asset_bank = bank_map.get_ref_mut(&asset_bank_index)?;

        update_bank_balances(
            asset_transfer,
            &BankBalanceType::Borrow,
            &mut asset_bank,
            user.get_bank_balance_mut(asset_bank_index).unwrap(),
        )?;

        update_bank_balances(
            asset_transfer,
            &BankBalanceType::Deposit,
            &mut asset_bank,
            liquidator.get_bank_balance_mut(asset_bank_index).unwrap(),
        )?;
    }

    if liability_transfer >= liability_transfer_to_cover_margin_shortage {
        user.being_liquidated = false;
    }

    let liquidator_meets_initial_margin_requirement =
        meets_initial_margin_requirement(liquidator, market_map, bank_map, oracle_map)?;

    validate!(
        liquidator_meets_initial_margin_requirement,
        ErrorCode::InsufficientCollateral,
        "Liquidator doesnt have enough collateral to take over borrow"
    )?;

    Ok(())
}

pub fn liquidate_borrow_for_perp_pnl(
    perp_market_index: u64,
    liability_bank_index: u64,
    liquidator_max_liability_transfer: u128,
    user: &mut User,
    user_key: &Pubkey,
    liquidator: &mut User,
    liquidator_key: &Pubkey,
    market_map: &MarketMap,
    bank_map: &BankMap,
    oracle_map: &mut OracleMap,
    now: i64,
    liquidation_margin_buffer_ratio: u8,
) -> ClearingHouseResult {
    user.get_position(perp_market_index).map_err(|e| {
        msg!(
            "User does not have a position for perp market {}",
            perp_market_index
        );
        e
    })?;

    user.get_bank_balance(liability_bank_index).ok_or_else(|| {
        msg!(
            "User does not have a bank balance for liability bank {}",
            liability_bank_index
        );
        ErrorCode::CouldNotFindBankBalance
    })?;

    liquidator
        .force_get_position_mut(perp_market_index)
        .map_err(|e| {
            msg!("Liquidator has no available positions to take on pnl");
            e
        })?;

    match liquidator.get_bank_balance_mut(liability_bank_index) {
        Some(_) => {}
        None => {
            liquidator
                .add_bank_balance(liability_bank_index, BankBalanceType::Borrow)
                .map_err(|e| {
                    msg!("Liquidator has no available bank balances to take on borrow");
                    e
                })?;
        }
    };

    settle_funding_payment(
        user,
        user_key,
        market_map.get_ref_mut(&perp_market_index)?.deref_mut(),
        now,
    )?;

    settle_funding_payment(
        liquidator,
        liquidator_key,
        market_map.get_ref_mut(&perp_market_index)?.deref_mut(),
        now,
    )?;

    let (pnl, quote_price, quote_decimals, pnl_asset_weight, pnl_liquidation_multiplier) = {
        let user_position = user.get_position(perp_market_index).unwrap();

        let base_asset_amount = user_position.base_asset_amount;

        validate!(
            base_asset_amount == 0,
            ErrorCode::InvalidPerpPositionToLiquidate,
            "Cant have open perp position"
        )?;

        validate!(
            user_position.open_orders == 0,
            ErrorCode::InvalidPerpPositionToLiquidate,
            "Cant have open orders for perp position"
        )?;

        let unsettled_pnl = user_position.unsettled_pnl;

        validate!(
            unsettled_pnl > 0,
            ErrorCode::InvalidPerpPositionToLiquidate,
            "Perp position must have position pnl"
        )?;

        let pnl_price = oracle_map.quote_asset_price_data.price;

        let market = market_map.get_ref(&perp_market_index)?;

        (
            unsettled_pnl.unsigned_abs(),
            pnl_price,
            6_u8,
            market.unsettled_maintenance_asset_weight, // TODO add market unsettled pnl weight
            LIQUIDATION_FEE_PRECISION
                .checked_add(market.liquidation_fee)
                .ok_or_else(math_error!())?,
        )
    };

    let (
        liability_amount,
        liability_price,
        liability_decimals,
        liability_weight,
        liability_liquidation_multiplier,
    ) = {
        let mut liability_bank = bank_map.get_ref_mut(&liability_bank_index)?;
        update_bank_cumulative_interest(&mut liability_bank, now)?;

        let user_bank_balance = user.get_bank_balance(liability_bank_index).unwrap();

        validate!(
            user_bank_balance.balance_type == BankBalanceType::Borrow,
            ErrorCode::WrongBankBalanceType,
            "User did not have a deposit for the borrow bank index"
        )?;

        let token_amount = get_token_amount(
            user_bank_balance.balance,
            &liability_bank,
            &user_bank_balance.balance_type,
        )?;

        // TODO add oracle checks
        let token_price = oracle_map.get_price_data(&liability_bank.oracle)?.price;

        (
            token_amount,
            token_price,
            liability_bank.decimals,
            liability_bank.maintenance_liability_weight,
            LIQUIDATION_FEE_PRECISION
                .checked_sub(liability_bank.liquidation_fee)
                .ok_or_else(math_error!())?,
        )
    };

    let (margin_requirement, total_collateral) = calculate_margin_requirement_and_total_collateral(
        user,
        market_map,
        MarginRequirementType::Maintenance,
        bank_map,
        oracle_map,
    )?;

    let margin_requirement_plus_buffer =
        get_margin_requirement_plus_buffer(margin_requirement, liquidation_margin_buffer_ratio)?;

    if !user.being_liquidated && total_collateral >= cast(margin_requirement)? {
        return Err(ErrorCode::SufficientCollateral);
    } else if user.being_liquidated && total_collateral >= cast(margin_requirement_plus_buffer)? {
        user.being_liquidated = false;
        return Ok(());
    }

    let margin_shortage = cast_to_i128(margin_requirement_plus_buffer)?
        .checked_sub(total_collateral)
        .ok_or_else(math_error!())?
        .unsigned_abs();

    // Determine what amount of borrow to transfer to reduce margin shortage to 0
    let liability_transfer_to_cover_margin_shortage =
        calculate_liability_transfer_to_cover_margin_shortage(
            margin_shortage,
            pnl_asset_weight as u128,
            pnl_liquidation_multiplier,
            liability_weight,
            liability_liquidation_multiplier,
            liability_decimals,
            liability_price,
        )?;

    // Given the user's deposit amount, how much borrow can be transferred?
    let liability_transfer_implied_by_pnl = calculate_liability_transfer_implied_by_asset_amount(
        pnl,
        pnl_liquidation_multiplier,
        quote_decimals,
        quote_price,
        liability_liquidation_multiplier,
        liability_decimals,
        liability_price,
    )?;

    let liability_transfer = liquidator_max_liability_transfer
        .min(liability_amount)
        .min(liability_transfer_to_cover_margin_shortage)
        .min(liability_transfer_implied_by_pnl);

    // Given the borrow amount to transfer, determine how much deposit amount to transfer
    let pnl_transfer = calculate_asset_transfer_for_liability_transfer(
        pnl,
        pnl_liquidation_multiplier,
        quote_decimals,
        quote_price,
        liability_transfer,
        liability_liquidation_multiplier,
        liability_decimals,
        liability_price,
    )?;

    {
        let mut liability_bank = bank_map.get_ref_mut(&liability_bank_index)?;

        update_bank_balances(
            liability_transfer,
            &BankBalanceType::Deposit,
            &mut liability_bank,
            user.get_bank_balance_mut(liability_bank_index).unwrap(),
        )?;

        update_bank_balances(
            liability_transfer,
            &BankBalanceType::Borrow,
            &mut liability_bank,
            liquidator
                .get_bank_balance_mut(liability_bank_index)
                .unwrap(),
        )?;
    }

    {
        let mut market = market_map.get_ref_mut(&perp_market_index)?;

        let liquidator_position = liquidator.force_get_position_mut(perp_market_index)?;
        update_unsettled_pnl(
            liquidator_position,
            &mut market,
            cast_to_i128(pnl_transfer)?,
        )?;

        let user_position = user.get_position_mut(perp_market_index)?;
        update_unsettled_pnl(user_position, &mut market, -cast_to_i128(pnl_transfer)?)?;
    }

    if liability_transfer >= liability_transfer_to_cover_margin_shortage {
        user.being_liquidated = false;
    }

    let liquidator_meets_initial_margin_requirement =
        meets_initial_margin_requirement(liquidator, market_map, bank_map, oracle_map)?;

    validate!(
        liquidator_meets_initial_margin_requirement,
        ErrorCode::InsufficientCollateral,
        "Liquidator doesnt have enough collateral to take over borrow"
    )?;

    Ok(())
}

pub fn liquidate_perp_pnl_for_deposit(
    perp_market_index: u64,
    asset_bank_index: u64,
    liquidator_max_pnl_transfer: u128,
    user: &mut User,
    user_key: &Pubkey,
    liquidator: &mut User,
    liquidator_key: &Pubkey,
    market_map: &MarketMap,
    bank_map: &BankMap,
    oracle_map: &mut OracleMap,
    now: i64,
    liquidation_margin_buffer_ratio: u8,
) -> ClearingHouseResult {
    user.get_position(perp_market_index).map_err(|e| {
        msg!(
            "User does not have a position for perp market {}",
            perp_market_index
        );
        e
    })?;

    user.get_bank_balance(asset_bank_index).ok_or_else(|| {
        msg!(
            "User does not have a bank balance for deposit bank {}",
            asset_bank_index
        );
        ErrorCode::CouldNotFindBankBalance
    })?;

    liquidator
        .force_get_position_mut(perp_market_index)
        .map_err(|e| {
            msg!("Liquidator has no available positions to take on pnl");
            e
        })?;

    match liquidator.get_bank_balance_mut(asset_bank_index) {
        Some(_) => {}
        None => {
            liquidator
                .add_bank_balance(asset_bank_index, BankBalanceType::Borrow)
                .map_err(|e| {
                    msg!("Liquidator has no available bank balances to take on deposit");
                    e
                })?;
        }
    };

    settle_funding_payment(
        user,
        user_key,
        market_map.get_ref_mut(&perp_market_index)?.deref_mut(),
        now,
    )?;

    settle_funding_payment(
        liquidator,
        liquidator_key,
        market_map.get_ref_mut(&perp_market_index)?.deref_mut(),
        now,
    )?;

    let (asset_amount, asset_price, asset_decimals, asset_weight, asset_liquidation_multiplier) = {
        let mut asset_bank = bank_map.get_ref_mut(&asset_bank_index)?;
        update_bank_cumulative_interest(&mut asset_bank, now)?;

        let user_bank_balance = user.get_bank_balance(asset_bank_index).unwrap();

        validate!(
            user_bank_balance.balance_type == BankBalanceType::Deposit,
            ErrorCode::WrongBankBalanceType,
            "User did not have a deposit for the borrow bank index"
        )?;

        let token_amount = get_token_amount(
            user_bank_balance.balance,
            &asset_bank,
            &user_bank_balance.balance_type,
        )?;

        // TODO add oracle checks
        let token_price = oracle_map.get_price_data(&asset_bank.oracle)?.price;

        (
            token_amount,
            token_price,
            asset_bank.decimals,
            asset_bank.maintenance_asset_weight,
            LIQUIDATION_FEE_PRECISION
                .checked_add(asset_bank.liquidation_fee)
                .ok_or_else(math_error!())?,
        )
    };

    let (
        unsettled_pnl,
        quote_price,
        quote_decimals,
        pnl_liability_weight,
        pnl_liquidation_multiplier,
    ) = {
        let user_position = user.get_position(perp_market_index).unwrap();

        let base_asset_amount = user_position.base_asset_amount;

        validate!(
            base_asset_amount == 0,
            ErrorCode::InvalidPerpPositionToLiquidate,
            "Cant have open perp position"
        )?;

        validate!(
            user_position.open_orders == 0,
            ErrorCode::InvalidPerpPositionToLiquidate,
            "Cant have open orders on perp position"
        )?;

        let unsettled_pnl = user_position.unsettled_pnl;

        validate!(
            unsettled_pnl < 0,
            ErrorCode::InvalidPerpPositionToLiquidate,
            "Perp position must have negative pnl"
        )?;

        let pnl_price = oracle_map.quote_asset_price_data.price;

        let market = market_map.get_ref(&perp_market_index)?;

        (
            unsettled_pnl.unsigned_abs(),
            pnl_price,
            6_u8,
            BANK_WEIGHT_PRECISION,
            LIQUIDATION_FEE_PRECISION
                .checked_sub(market.liquidation_fee)
                .ok_or_else(math_error!())?,
        )
    };

    let (margin_requirement, total_collateral) = calculate_margin_requirement_and_total_collateral(
        user,
        market_map,
        MarginRequirementType::Maintenance,
        bank_map,
        oracle_map,
    )?;

    let margin_requirement_plus_buffer =
        get_margin_requirement_plus_buffer(margin_requirement, liquidation_margin_buffer_ratio)?;

    if !user.being_liquidated && total_collateral >= cast(margin_requirement)? {
        return Err(ErrorCode::SufficientCollateral);
    } else if user.being_liquidated && total_collateral >= cast(margin_requirement_plus_buffer)? {
        user.being_liquidated = false;
        return Ok(());
    }

    let margin_shortage = cast_to_i128(margin_requirement_plus_buffer)?
        .checked_sub(total_collateral)
        .ok_or_else(math_error!())?
        .unsigned_abs();

    // Determine what amount of borrow to transfer to reduce margin shortage to 0
    let pnl_transfer_to_cover_margin_shortage =
        calculate_liability_transfer_to_cover_margin_shortage(
            margin_shortage,
            asset_weight,
            asset_liquidation_multiplier,
            pnl_liability_weight,
            pnl_liquidation_multiplier,
            quote_decimals,
            quote_price,
        )?;

    // Given the user's deposit amount, how much borrow can be transferred?
    let pnl_transfer_implied_by_asset_amount =
        calculate_liability_transfer_implied_by_asset_amount(
            asset_amount,
            asset_liquidation_multiplier,
            asset_decimals,
            asset_price,
            pnl_liquidation_multiplier,
            quote_decimals,
            quote_price,
        )?;

    let pnl_transfer = liquidator_max_pnl_transfer
        .min(unsettled_pnl)
        .min(pnl_transfer_to_cover_margin_shortage)
        .min(pnl_transfer_implied_by_asset_amount);

    // Given the borrow amount to transfer, determine how much deposit amount to transfer
    let asset_transfer = calculate_asset_transfer_for_liability_transfer(
        asset_amount,
        asset_liquidation_multiplier,
        asset_decimals,
        asset_price,
        pnl_transfer,
        pnl_liquidation_multiplier,
        quote_decimals,
        quote_price,
    )?;

    {
        let mut asset_bank = bank_map.get_ref_mut(&asset_bank_index)?;

        update_bank_balances(
            asset_transfer,
            &BankBalanceType::Borrow,
            &mut asset_bank,
            user.get_bank_balance_mut(asset_bank_index).unwrap(),
        )?;

        update_bank_balances(
            asset_transfer,
            &BankBalanceType::Deposit,
            &mut asset_bank,
            liquidator.get_bank_balance_mut(asset_bank_index).unwrap(),
        )?;
    }

    {
        let mut market = market_map.get_ref_mut(&perp_market_index)?;

        let liquidator_position = liquidator.force_get_position_mut(perp_market_index)?;
        update_unsettled_pnl(
            liquidator_position,
            &mut market,
            -cast_to_i128(pnl_transfer)?,
        )?;

        let user_position = user.get_position_mut(perp_market_index)?;
        update_unsettled_pnl(user_position, &mut market, cast_to_i128(pnl_transfer)?)?;
    }

    if pnl_transfer >= pnl_transfer_to_cover_margin_shortage {
        user.being_liquidated = false;
    }

    let liquidator_meets_initial_margin_requirement =
        meets_initial_margin_requirement(liquidator, market_map, bank_map, oracle_map)?;

    validate!(
        liquidator_meets_initial_margin_requirement,
        ErrorCode::InsufficientCollateral,
        "Liquidator doesnt have enough collateral to take over borrow"
    )?;

    Ok(())
}
