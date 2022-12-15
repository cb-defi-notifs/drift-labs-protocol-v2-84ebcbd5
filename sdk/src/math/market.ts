import { BN } from '@project-serum/anchor';
import {
	PerpMarketAccount,
	PositionDirection,
	MarginCategory,
	SpotMarketAccount,
	SpotBalanceType,
	MarketType,
	isVariant,
} from '../types';
import {
	calculateAmmReservesAfterSwap,
	calculatePrice,
	calculateUpdatedAMMSpreadReserves,
	getSwapDirection,
	calculateUpdatedAMM,
} from './amm';
import {
	calculateSizeDiscountAssetWeight,
	calculateSizePremiumLiabilityWeight,
} from './margin';
import { OraclePriceData } from '../oracles/types';
import {
	BASE_PRECISION,
	MARGIN_PRECISION,
	PRICE_TO_QUOTE_PRECISION,
	ZERO,
} from '../constants/numericConstants';
import { getTokenAmount } from './spotBalance';

/**
 * Calculates market mark price
 *
 * @param market
 * @return markPrice : Precision PRICE_PRECISION
 */
export function calculateReservePrice(
	market: PerpMarketAccount,
	oraclePriceData: OraclePriceData
): BN {
	const newAmm = calculateUpdatedAMM(market.amm, oraclePriceData);
	return calculatePrice(
		newAmm.baseAssetReserve,
		newAmm.quoteAssetReserve,
		newAmm.pegMultiplier
	);
}

/**
 * Calculates market bid price
 *
 * @param market
 * @return bidPrice : Precision PRICE_PRECISION
 */
export function calculateBidPrice(
	market: PerpMarketAccount,
	oraclePriceData: OraclePriceData
): BN {
	const { baseAssetReserve, quoteAssetReserve, newPeg } =
		calculateUpdatedAMMSpreadReserves(
			market.amm,
			PositionDirection.SHORT,
			oraclePriceData
		);

	return calculatePrice(baseAssetReserve, quoteAssetReserve, newPeg);
}

/**
 * Calculates market ask price
 *
 * @param market
 * @return askPrice : Precision PRICE_PRECISION
 */
export function calculateAskPrice(
	market: PerpMarketAccount,
	oraclePriceData: OraclePriceData
): BN {
	const { baseAssetReserve, quoteAssetReserve, newPeg } =
		calculateUpdatedAMMSpreadReserves(
			market.amm,
			PositionDirection.LONG,
			oraclePriceData
		);

	return calculatePrice(baseAssetReserve, quoteAssetReserve, newPeg);
}

export function calculateNewMarketAfterTrade(
	baseAssetAmount: BN,
	direction: PositionDirection,
	market: PerpMarketAccount
): PerpMarketAccount {
	const [newQuoteAssetReserve, newBaseAssetReserve] =
		calculateAmmReservesAfterSwap(
			market.amm,
			'base',
			baseAssetAmount.abs(),
			getSwapDirection('base', direction)
		);

	const newAmm = Object.assign({}, market.amm);
	const newMarket = Object.assign({}, market);
	newMarket.amm = newAmm;
	newMarket.amm.quoteAssetReserve = newQuoteAssetReserve;
	newMarket.amm.baseAssetReserve = newBaseAssetReserve;

	return newMarket;
}

export function calculateOracleReserveSpread(
	market: PerpMarketAccount,
	oraclePriceData: OraclePriceData
): BN {
	const reservePrice = calculateReservePrice(market, oraclePriceData);
	return calculateOracleSpread(reservePrice, oraclePriceData);
}

export function calculateOracleSpread(
	price: BN,
	oraclePriceData: OraclePriceData
): BN {
	return price.sub(oraclePriceData.price);
}

export function getSpotMarketMarginRatio(
	market: SpotMarketAccount,
	marginCategory: MarginCategory
): number {
	const liabilityWeight =
		marginCategory == 'Initial'
			? market.initialLiabilityWeight
			: market.maintenanceLiabilityWeight;
	return 1 / (liabilityWeight / 10000 - 1);
}

export function calculateMarketMarginRatio(
	market: PerpMarketAccount | SpotMarketAccount,
	marketType: MarketType,
	size: BN,
	marginCategory: MarginCategory
): number {
	let marginRatio;
	switch (marginCategory) {
		case 'Initial':
			const marginRatioInitial = isVariant(marketType, 'perp')
				? (market as PerpMarketAccount).marginRatioInitial
				: getSpotMarketMarginRatio(market as SpotMarketAccount, marginCategory);

			marginRatio = calculateSizePremiumLiabilityWeight(
				size,
				new BN(market.imfFactor),
				new BN(marginRatioInitial),
				MARGIN_PRECISION
			).toNumber();
			break;
		case 'Maintenance':
			const marginRatioMaintenance = isVariant(marketType, 'perp')
				? (market as PerpMarketAccount).marginRatioMaintenance
				: getSpotMarketMarginRatio(market as SpotMarketAccount, marginCategory);

			marginRatio = calculateSizePremiumLiabilityWeight(
				size,
				new BN(market.imfFactor),
				new BN(marginRatioMaintenance),
				MARGIN_PRECISION
			).toNumber();
			break;
	}

	return marginRatio;
}

export function calculateUnrealizedAssetWeight(
	market: PerpMarketAccount,
	quoteSpotMarket: SpotMarketAccount,
	unrealizedPnl: BN,
	marginCategory: MarginCategory,
	oraclePriceData: OraclePriceData
): BN {
	let assetWeight: BN;
	switch (marginCategory) {
		case 'Initial':
			assetWeight = new BN(market.unrealizedPnlInitialAssetWeight);

			if (market.unrealizedPnlMaxImbalance.gt(ZERO)) {
				const netUnsettledPnl = calculateNetUserPnlImbalance(
					market,
					quoteSpotMarket,
					oraclePriceData
				);
				if (netUnsettledPnl.gt(market.unrealizedPnlMaxImbalance)) {
					assetWeight = assetWeight
						.mul(market.unrealizedPnlMaxImbalance)
						.div(netUnsettledPnl);
				}
			}

			assetWeight = calculateSizeDiscountAssetWeight(
				unrealizedPnl,
				new BN(market.unrealizedPnlImfFactor),
				assetWeight
			);
			break;
		case 'Maintenance':
			assetWeight = new BN(market.unrealizedPnlMaintenanceAssetWeight);
			break;
	}

	return assetWeight;
}

export function calculateMarketAvailablePNL(
	perpMarket: PerpMarketAccount,
	spotMarket: SpotMarketAccount
): BN {
	return getTokenAmount(
		perpMarket.pnlPool.scaledBalance,
		spotMarket,
		SpotBalanceType.DEPOSIT
	);
}

export function calculateNetUserPnl(
	perpMarket: PerpMarketAccount,
	oraclePriceData: OraclePriceData
): BN {
	const netUserPositionValue = perpMarket.amm.baseAssetAmountWithAmm
		.mul(oraclePriceData.price)
		.div(BASE_PRECISION)
		.div(PRICE_TO_QUOTE_PRECISION);

	const netUserCostBasis = perpMarket.amm.quoteAssetAmount;

	const netUserPnl = netUserPositionValue.add(netUserCostBasis);

	return netUserPnl;
}

export function calculateNetUserPnlImbalance(
	perpMarket: PerpMarketAccount,
	spotMarket: SpotMarketAccount,
	oraclePriceData: OraclePriceData
): BN {
	const netUserPnl = calculateNetUserPnl(perpMarket, oraclePriceData);

	const pnlPool = getTokenAmount(
		perpMarket.pnlPool.scaledBalance,
		spotMarket,
		SpotBalanceType.DEPOSIT
	);

	const imbalance = netUserPnl.sub(pnlPool);

	return imbalance;
}
