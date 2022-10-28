use crate::error::DriftResult;
use crate::math::auction::is_auction_complete;
use crate::state::fulfillment::{PerpFulfillmentMethod, SpotFulfillmentMethod};
use crate::state::user::Order;

pub fn determine_perp_fulfillment_methods(
    taker_order: &Order,
    maker_available: bool,
    amm_available: bool,
    slot: u64,
) -> DriftResult<Vec<PerpFulfillmentMethod>> {
    let mut fulfillment_methods = vec![];

    if maker_available {
        fulfillment_methods.push(PerpFulfillmentMethod::Match)
    }

    if amm_available && is_auction_complete(taker_order.slot, taker_order.auction_duration, slot)? {
        fulfillment_methods.push(PerpFulfillmentMethod::AMM)
    }

    Ok(fulfillment_methods)
}

pub fn determine_spot_fulfillment_methods(
    taker_order: &Order,
    maker_available: bool,
    serum_fulfillment_params_available: bool,
    slot: u64,
) -> DriftResult<Vec<SpotFulfillmentMethod>> {
    let mut fulfillment_methods = vec![];

    if maker_available {
        fulfillment_methods.push(SpotFulfillmentMethod::Match)
    }

    if !taker_order.post_only
        && serum_fulfillment_params_available
        && is_auction_complete(taker_order.slot, taker_order.auction_duration, slot)?
    {
        fulfillment_methods.push(SpotFulfillmentMethod::SerumV3)
    }

    Ok(fulfillment_methods)
}
