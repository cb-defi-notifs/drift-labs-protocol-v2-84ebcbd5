import {
	BankAccount,
	BankBalanceType,
	isVariant,
	MarginCategory,
} from '../types';
import { BN } from '@project-serum/anchor';
import {
	BANK_UTILIZATION_PRECISION,
	ONE,
	TEN,
	ZERO,
	BANK_INTEREST_PRECISION,
	BANK_WEIGHT_PRECISION,
	ONE_YEAR,
	AMM_RESERVE_PRECISION,
} from '../constants/numericConstants';
import {
	calculateSizeDiscountAssetWeight,
	calculateSizePremiumLiabilityWeight,
} from './margin';

export function getBalance(
	tokenAmount: BN,
	bank: BankAccount,
	balanceType: BankBalanceType
): BN {
	const precisionIncrease = TEN.pow(new BN(16 - bank.decimals));

	const cumulativeInterest = isVariant(balanceType, 'deposit')
		? bank.cumulativeDepositInterest
		: bank.cumulativeBorrowInterest;

	let balance = tokenAmount.mul(precisionIncrease).div(cumulativeInterest);

	if (!balance.eq(ZERO) && isVariant(balanceType, 'borrow')) {
		balance = balance.add(ONE);
	}

	return balance;
}

export function getTokenAmount(
	balanceAmount: BN,
	bank: BankAccount,
	balanceType: BankBalanceType
): BN {
	const precisionDecrease = TEN.pow(new BN(16 - bank.decimals));

	const cumulativeInterest = isVariant(balanceType, 'deposit')
		? bank.cumulativeDepositInterest
		: bank.cumulativeBorrowInterest;

	return balanceAmount.mul(cumulativeInterest).div(precisionDecrease);
}

export function calculateAssetWeight(
	balanceAmount: BN,
	bank: BankAccount,
	marginCategory: MarginCategory
): BN {
	const sizePrecision = TEN.pow(new BN(bank.decimals));
	let sizeInAmmReservePrecision;
	if (sizePrecision.gt(AMM_RESERVE_PRECISION)) {
		sizeInAmmReservePrecision = balanceAmount.div(
			sizePrecision.div(AMM_RESERVE_PRECISION)
		);
	} else {
		sizeInAmmReservePrecision = balanceAmount
			.mul(AMM_RESERVE_PRECISION)
			.div(sizePrecision);
	}

	let assetWeight;

	switch (marginCategory) {
		case 'Initial':
			assetWeight = calculateSizeDiscountAssetWeight(
				sizeInAmmReservePrecision,
				bank.imfFactor,
				bank.initialAssetWeight
			);
			break;
		case 'Maintenance':
			assetWeight = calculateSizeDiscountAssetWeight(
				sizeInAmmReservePrecision,
				bank.imfFactor,
				bank.maintenanceAssetWeight
			);
			break;
		default:
			assetWeight = bank.initialAssetWeight;
			break;
	}

	return assetWeight;
}

export function calculateLiabilityWeight(
	balanceAmount: BN,
	bank: BankAccount,
	marginCategory: MarginCategory
): BN {
	const sizePrecision = TEN.pow(new BN(bank.decimals));
	let sizeInAmmReservePrecision;
	if (sizePrecision.gt(AMM_RESERVE_PRECISION)) {
		sizeInAmmReservePrecision = balanceAmount.div(
			sizePrecision.div(AMM_RESERVE_PRECISION)
		);
	} else {
		sizeInAmmReservePrecision = balanceAmount
			.mul(AMM_RESERVE_PRECISION)
			.div(sizePrecision);
	}

	let assetWeight;

	switch (marginCategory) {
		case 'Initial':
			assetWeight = calculateSizePremiumLiabilityWeight(
				sizeInAmmReservePrecision,
				bank.imfFactor,
				bank.initialLiabilityWeight,
				BANK_WEIGHT_PRECISION
			);
			break;
		case 'Maintenance':
			assetWeight = calculateSizePremiumLiabilityWeight(
				sizeInAmmReservePrecision,
				bank.imfFactor,
				bank.maintenanceLiabilityWeight,
				BANK_WEIGHT_PRECISION
			);
			break;
		default:
			assetWeight = bank.initialLiabilityWeight;
			break;
	}

	return assetWeight;
}

export function calculateUtilization(bank: BankAccount): BN {
	const tokenDepositAmount = getTokenAmount(
		bank.depositBalance,
		bank,
		BankBalanceType.DEPOSIT
	);
	const tokenBorrowAmount = getTokenAmount(
		bank.borrowBalance,
		bank,
		BankBalanceType.BORROW
	);

	let utilization: BN;
	if (tokenBorrowAmount.eq(ZERO) && tokenDepositAmount.eq(ZERO)) {
		utilization = ZERO;
	} else if (tokenDepositAmount.eq(ZERO)) {
		utilization = BANK_UTILIZATION_PRECISION;
	} else {
		utilization = tokenBorrowAmount
			.mul(BANK_UTILIZATION_PRECISION)
			.div(tokenDepositAmount);
	}

	return utilization;
}

export function calculateInterestRate(bank: BankAccount): BN {
	const utilization = calculateUtilization(bank);

	let interestRate: BN;
	if (utilization.gt(bank.optimalUtilization)) {
		const surplusUtilization = utilization.sub(bank.optimalUtilization);
		const borrowRateSlope = bank.maxBorrowRate
			.sub(bank.optimalBorrowRate)
			.mul(BANK_UTILIZATION_PRECISION)
			.div(BANK_UTILIZATION_PRECISION.sub(bank.optimalUtilization));

		interestRate = bank.optimalBorrowRate.add(
			surplusUtilization.mul(borrowRateSlope).div(BANK_UTILIZATION_PRECISION)
		);
	} else {
		const borrowRateSlope = bank.optimalBorrowRate
			.mul(BANK_UTILIZATION_PRECISION)
			.div(BANK_UTILIZATION_PRECISION.sub(bank.optimalUtilization));

		interestRate = utilization
			.mul(borrowRateSlope)
			.div(BANK_UTILIZATION_PRECISION);
	}

	return interestRate;
}

export function calculateDepositRate(bank: BankAccount): BN {
	const utilization = calculateUtilization(bank);
	const borrowRate = calculateBorrowRate(bank);
	const depositRate = borrowRate
		.mul(utilization)
		.div(BANK_UTILIZATION_PRECISION);
	return depositRate;
}

export function calculateBorrowRate(bank: BankAccount): BN {
	return calculateInterestRate(bank);
}

export function calculateInterestAccumulated(
	bank: BankAccount,
	now: BN
): { borrowInterest: BN; depositInterest: BN } {
	const interestRate = calculateInterestRate(bank);

	const timeSinceLastUpdate = now.sub(bank.lastInterestTs);

	const modifiedBorrowRate = interestRate.mul(timeSinceLastUpdate);

	const utilization = calculateUtilization(bank);

	const modifiedDepositRate = modifiedBorrowRate
		.mul(utilization)
		.div(BANK_UTILIZATION_PRECISION);

	const borrowInterest = bank.cumulativeBorrowInterest
		.mul(modifiedBorrowRate)
		.div(ONE_YEAR)
		.div(BANK_INTEREST_PRECISION)
		.add(ONE);
	const depositInterest = bank.cumulativeDepositInterest
		.mul(modifiedDepositRate)
		.div(ONE_YEAR)
		.div(BANK_INTEREST_PRECISION);

	return { borrowInterest, depositInterest };
}

export function calculateWithdrawLimit(
	bank: BankAccount,
	now: BN
): { borrowLimit: BN; withdrawLimit: BN } {
	const bankDepositTokenAmount = getTokenAmount(
		bank.depositBalance,
		bank,
		BankBalanceType.DEPOSIT
	);
	const bankBorrowTokenAmount = getTokenAmount(
		bank.borrowBalance,
		bank,
		BankBalanceType.BORROW
	);

	const twentyFourHours = new BN(60 * 60 * 24);
	const sinceLast = now.sub(bank.lastTwapTs);
	const sinceStart = BN.max(ZERO, twentyFourHours.sub(sinceLast));
	const borrowTokenTwapLive = bank.borrowTokenTwap
		.mul(sinceStart)
		.add(bankBorrowTokenAmount.mul(sinceLast))
		.div(sinceLast.add(sinceLast));

	const depositTokenTwapLive = bank.depositTokenTwap
		.mul(sinceStart)
		.add(bankDepositTokenAmount.mul(sinceLast))
		.div(sinceLast.add(sinceLast));

	const maxBorrowTokens = BN.min(
		BN.max(
			bankDepositTokenAmount.div(new BN(6)),
			borrowTokenTwapLive.add(borrowTokenTwapLive.div(new BN(5)))
		),
		bankDepositTokenAmount.sub(bankDepositTokenAmount.div(new BN(10)))
	); // between ~15-90% utilization with friction on twap

	const minDepositTokens = depositTokenTwapLive.sub(
		BN.min(
			BN.max(depositTokenTwapLive.div(new BN(5)), bank.withdrawGuardThreshold),
			depositTokenTwapLive
		)
	);

	return {
		borrowLimit: maxBorrowTokens.sub(bankBorrowTokenAmount),
		withdrawLimit: bankDepositTokenAmount.sub(minDepositTokens),
	};
}
