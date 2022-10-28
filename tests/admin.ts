import * as anchor from '@project-serum/anchor';
import { Program } from '@project-serum/anchor';
import { BN } from '../sdk';
import { assert } from 'chai';

import { AdminClient, OracleGuardRails } from '../sdk/src';
import { OracleSource } from '../sdk';

import {
	mockOracle,
	mockUSDCMint,
	initializeQuoteSpotMarket,
} from './testHelpers';
import { PublicKey } from '@solana/web3.js';

describe('admin', () => {
	const provider = anchor.AnchorProvider.local(undefined, {
		commitment: 'confirmed',
		preflightCommitment: 'confirmed',
	});
	const connection = provider.connection;
	anchor.setProvider(provider);
	const driftProgram = anchor.workspace.Drift as Program;

	let driftClient: AdminClient;

	let usdcMint;

	before(async () => {
		usdcMint = await mockUSDCMint(provider);

		driftClient = new AdminClient({
			connection,
			wallet: provider.wallet,
			programID: driftProgram.programId,
			opts: {
				commitment: 'confirmed',
			},
			activeUserId: 0,
			perpMarketIndexes: [0],
			spotMarketIndexes: [0],
		});

		await driftClient.initialize(usdcMint.publicKey, true);
		await driftClient.subscribe();

		await initializeQuoteSpotMarket(driftClient, usdcMint.publicKey);
		await driftClient.updatePerpAuctionDuration(new BN(0));

		const solUsd = await mockOracle(1);
		const periodicity = new BN(60 * 60); // 1 HOUR

		await driftClient.initializeMarket(
			solUsd,
			new BN(1000),
			new BN(1000),
			periodicity
		);
	});

	it('Update Amm Jit', async () => {
		await driftClient.fetchAccounts();
		assert(driftClient.getPerpMarketAccount(0).amm.ammJitIntensity == 0);

		await driftClient.updateAmmJitIntensity(0, 100);
		await driftClient.fetchAccounts();
		assert(driftClient.getPerpMarketAccount(0).amm.ammJitIntensity == 100);

		await driftClient.updateAmmJitIntensity(0, 50);
		await driftClient.fetchAccounts();
		assert(driftClient.getPerpMarketAccount(0).amm.ammJitIntensity == 50);
	});

	it('Update Margin Ratio', async () => {
		const marginRatioInitial = 3000;
		const marginRatioMaintenance = 1000;

		await driftClient.updateMarginRatio(
			0,
			marginRatioInitial,
			marginRatioMaintenance
		);

		await driftClient.fetchAccounts();
		const market = driftClient.getPerpMarketAccount(0);

		assert(market.marginRatioInitial === marginRatioInitial);
		assert(market.marginRatioMaintenance === marginRatioMaintenance);
	});

	it('Update perp fee structure', async () => {
		const newFeeStructure = driftClient.getStateAccount().perpFeeStructure;
		newFeeStructure.flatFillerFee = new BN(0);

		await driftClient.updatePerpFeeStructure(newFeeStructure);

		await driftClient.fetchAccounts();
		const state = driftClient.getStateAccount();

		assert(
			JSON.stringify(newFeeStructure) === JSON.stringify(state.perpFeeStructure)
		);
	});

	it('Update spot fee structure', async () => {
		const newFeeStructure = driftClient.getStateAccount().spotFeeStructure;
		newFeeStructure.flatFillerFee = new BN(1);

		await driftClient.updateSpotFeeStructure(newFeeStructure);

		await driftClient.fetchAccounts();
		const state = driftClient.getStateAccount();

		assert(
			JSON.stringify(newFeeStructure) === JSON.stringify(state.spotFeeStructure)
		);
	});

	it('Update oracle guard rails', async () => {
		const oracleGuardRails: OracleGuardRails = {
			priceDivergence: {
				markOracleDivergenceNumerator: new BN(1),
				markOracleDivergenceDenominator: new BN(1),
			},
			validity: {
				slotsBeforeStaleForAmm: new BN(1),
				slotsBeforeStaleForMargin: new BN(1),
				confidenceIntervalMaxSize: new BN(1),
				tooVolatileRatio: new BN(1),
			},
			useForLiquidations: false,
		};

		await driftClient.updateOracleGuardRails(oracleGuardRails);

		await driftClient.fetchAccounts();
		const state = driftClient.getStateAccount();

		assert(
			JSON.stringify(oracleGuardRails) ===
				JSON.stringify(state.oracleGuardRails)
		);
	});

	it('Update protocol mint', async () => {
		const mint = new PublicKey('2fvh6hkCYfpNqke9N48x6HcrW92uZVU3QSiXZX4A5L27');

		await driftClient.updateDiscountMint(mint);

		await driftClient.fetchAccounts();
		const state = driftClient.getStateAccount();

		assert(state.discountMint.equals(mint));
	});

	// it('Update max deposit', async () => {
	// 	const maxDeposit = new BN(10);

	// 	await driftClient.updateMaxDeposit(maxDeposit);

	// 	await driftClient.fetchAccounts();
	// 	const state = driftClient.getStateAccount();

	// 	assert(state.maxDeposit.eq(maxDeposit));
	// });

	it('Update market oracle', async () => {
		const newOracle = PublicKey.default;
		const newOracleSource = OracleSource.SWITCHBOARD;

		await driftClient.updateMarketOracle(0, newOracle, newOracleSource);

		await driftClient.fetchAccounts();
		const market = driftClient.getPerpMarketAccount(0);
		assert(market.amm.oracle.equals(PublicKey.default));
		assert(
			JSON.stringify(market.amm.oracleSource) ===
				JSON.stringify(newOracleSource)
		);
	});

	it('Update market minimum quote asset trade size', async () => {
		const minimumTradeSize = new BN(1);

		await driftClient.updateMarketMinimumQuoteAssetTradeSize(
			0,
			minimumTradeSize
		);

		await driftClient.fetchAccounts();
		const market = driftClient.getPerpMarketAccount(0);
		assert(market.amm.minimumQuoteAssetTradeSize.eq(minimumTradeSize));
	});

	it('Update market base asset step size', async () => {
		const stepSize = new BN(2);

		await driftClient.updateMarketBaseAssetAmountStepSize(0, stepSize);

		await driftClient.fetchAccounts();
		const market = driftClient.getPerpMarketAccount(0);
		assert(market.amm.baseAssetAmountStepSize.eq(stepSize));
	});

	it('Pause funding', async () => {
		await driftClient.updateFundingPaused(true);
		await driftClient.fetchAccounts();
		const state = driftClient.getStateAccount();
		assert(state.fundingPaused);
	});

	it('Disable admin controls prices', async () => {
		let state = driftClient.getStateAccount();
		assert(state.adminControlsPrices);
		await driftClient.disableAdminControlsPrices();
		await driftClient.fetchAccounts();
		state = driftClient.getStateAccount();
		assert(!state.adminControlsPrices);
	});

	it('Update admin', async () => {
		const newAdminKey = PublicKey.default;

		await driftClient.updateAdmin(newAdminKey);

		await driftClient.fetchAccounts();
		const state = driftClient.getStateAccount();

		assert(state.admin.equals(newAdminKey));
	});

	after(async () => {
		await driftClient.unsubscribe();
	});
});
