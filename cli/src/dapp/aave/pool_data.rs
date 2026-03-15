//! Raw ABI interaction with Aave V3 UiPoolDataProvider.
//!
//! AggregatedReserveData has 45 fields which exceeds alloy's sol! macro tuple limit.
//! We use raw eth_call with manual ABI encode/decode via alloy primitives.

use alloy::primitives::{keccak256, Address, Bytes, I256, U256};
use alloy::providers::Provider;
use alloy::rpc::types::TransactionRequest;
use anyhow::{bail, Result};
use serde::Serialize;

/// Minimal representation of AggregatedReserveData fields we care about.
#[derive(Debug, Clone, Serialize)]
pub struct ReserveData {
    pub underlying_asset: Address,
    pub name: String,
    pub symbol: String,
    pub decimals: U256,
    pub base_ltv: U256,
    pub liquidation_threshold: U256,
    pub liquidation_bonus: U256,
    pub reserve_factor: U256,
    pub usage_as_collateral_enabled: bool,
    pub borrowing_enabled: bool,
    pub is_active: bool,
    pub is_frozen: bool,
    pub liquidity_rate: u128,
    pub variable_borrow_rate: u128,
    pub stable_borrow_rate: u128,
    pub total_a_token: U256,
    pub total_stable_debt: U256,
    pub total_variable_debt: U256,
    pub available_liquidity: U256,
    pub price_in_market_ref_currency: U256,
    pub is_paused: bool,
    pub supply_cap: U256,
    pub borrow_cap: U256,
    pub a_token_address: Address,
}

#[derive(Debug, Clone, Serialize)]
pub struct BaseCurrencyInfo {
    pub market_ref_currency_unit: U256,
    pub market_ref_currency_price_usd: I256,
    pub network_base_token_price_usd: I256,
    pub network_base_token_price_decimals: u8,
}

#[derive(Debug, Clone, Serialize)]
pub struct UserReserveData {
    pub underlying_asset: Address,
    pub scaled_a_token_balance: U256,
    pub usage_as_collateral: bool,
    pub stable_borrow_rate: U256,
    pub scaled_variable_debt: U256,
    pub principal_stable_debt: U256,
    pub stable_borrow_last_update: U256,
}

/// Call getReservesData(address) on UiPoolDataProvider and decode the result.
pub async fn get_reserves_data(
    provider: &impl Provider,
    ui_provider_addr: Address,
    pool_addr_provider: Address,
) -> Result<(Vec<ReserveData>, BaseCurrencyInfo)> {
    // getReservesData(address) selector
    let selector = &keccak256("getReservesData(address)")[..4];
    let mut calldata = Vec::with_capacity(36);
    calldata.extend_from_slice(selector);
    // ABI encode address (padded to 32 bytes)
    calldata.extend_from_slice(&[0u8; 12]);
    calldata.extend_from_slice(pool_addr_provider.as_slice());

    let tx = TransactionRequest::default()
        .to(ui_provider_addr)
        .input(Bytes::from(calldata).into())
        .gas_limit(20_000_000);

    let result = provider.call(tx).await?;
    decode_reserves_data(&result)
}

/// Call getUserReservesData(address,address) on UiPoolDataProvider.
pub async fn get_user_reserves_data(
    provider: &impl Provider,
    ui_provider_addr: Address,
    pool_addr_provider: Address,
    user: Address,
) -> Result<Vec<UserReserveData>> {
    let selector = &keccak256("getUserReservesData(address,address)")[..4];
    let mut calldata = Vec::with_capacity(68);
    calldata.extend_from_slice(selector);
    calldata.extend_from_slice(&[0u8; 12]);
    calldata.extend_from_slice(pool_addr_provider.as_slice());
    calldata.extend_from_slice(&[0u8; 12]);
    calldata.extend_from_slice(user.as_slice());

    let tx = TransactionRequest::default()
        .to(ui_provider_addr)
        .input(Bytes::from(calldata).into())
        .gas_limit(20_000_000);

    let result = provider.call(tx).await?;
    decode_user_reserves_data(&result)
}

// ── ABI decoding helpers ──

fn read_u256(data: &[u8], offset: usize) -> U256 {
    if offset + 32 > data.len() {
        return U256::ZERO;
    }
    U256::from_be_slice(&data[offset..offset + 32])
}

fn read_i256(data: &[u8], offset: usize) -> I256 {
    if offset + 32 > data.len() {
        return I256::ZERO;
    }
    I256::try_from_be_slice(&data[offset..offset + 32]).unwrap_or(I256::ZERO)
}

fn read_address(data: &[u8], offset: usize) -> Address {
    if offset + 32 > data.len() {
        return Address::ZERO;
    }
    Address::from_slice(&data[offset + 12..offset + 32])
}

fn read_bool(data: &[u8], offset: usize) -> bool {
    read_u256(data, offset) != U256::ZERO
}

fn read_u128(data: &[u8], offset: usize) -> u128 {
    let v = read_u256(data, offset);
    v.try_into().unwrap_or(u128::MAX)
}

fn read_u8(data: &[u8], offset: usize) -> u8 {
    let v = read_u256(data, offset);
    v.try_into().unwrap_or(u8::MAX)
}

fn read_string(data: &[u8], base: usize, str_rel_offset: usize) -> String {
    let str_offset = read_u256(data, str_rel_offset).to::<usize>();
    let abs_offset = base + str_offset;
    if abs_offset + 32 > data.len() {
        return String::new();
    }
    let len = read_u256(data, abs_offset).to::<usize>();
    let str_start = abs_offset + 32;
    if str_start + len > data.len() {
        return String::new();
    }
    String::from_utf8_lossy(&data[str_start..str_start + len]).to_string()
}

/// Decode the ABI-encoded result of getReservesData.
/// Returns: (AggregatedReserveData[], BaseCurrencyInfo)
/// ABI: tuple of (dynamic array offset, struct offset) at top level
fn decode_reserves_data(data: &[u8]) -> Result<(Vec<ReserveData>, BaseCurrencyInfo)> {
    if data.len() < 64 {
        bail!("getReservesData response too short");
    }

    // Top-level ABI layout for (AggregatedReserveData[] memory, BaseCurrencyInfo memory):
    // - Word 0: offset to dynamic array
    // - Words 1-4: BaseCurrencyInfo (static struct, encoded inline)
    let reserves_offset: usize = read_u256(data, 0)
        .try_into()
        .map_err(|_| anyhow::anyhow!("reserves offset overflow"))?;

    // BaseCurrencyInfo is inline at byte 32 (4 fields: uint256, int256, int256, uint8)
    let base_currency = BaseCurrencyInfo {
        market_ref_currency_unit: read_u256(data, 32),
        market_ref_currency_price_usd: read_i256(data, 64),
        network_base_token_price_usd: read_i256(data, 96),
        network_base_token_price_decimals: read_u8(data, 128),
    };

    // Decode reserves array
    if reserves_offset + 32 > data.len() {
        bail!("getReservesData: reserves offset out of bounds");
    }
    let array_len: usize = read_u256(data, reserves_offset)
        .try_into()
        .map_err(|_| anyhow::anyhow!("array length overflow"))?;
    if array_len > 10_000 {
        bail!("getReservesData: suspicious array length {}", array_len);
    }
    let mut reserves = Vec::with_capacity(array_len);

    // After array length, we have `array_len` offsets (each relative to reserves_offset)
    for i in 0..array_len {
        let elem_rel_offset_pos = reserves_offset + 32 + i * 32;
        let elem_rel_offset: usize = read_u256(data, elem_rel_offset_pos).try_into().unwrap_or(0);
        let elem_start = reserves_offset + 32 + elem_rel_offset;

        // UiPoolDataProviderV3 AggregatedReserveData — V3.1 layout (40 fixed fields).
        // V3.1 removed stableBorrowRateEnabled and stableBorrowRate from V3.0.
        // Also removed: totalPrincipalStableDebt, avgStableRate, stableDebtLastUpdate,
        //   stableRateSlope1/2, baseStableRate, interestRateStrategyAddress, eMode sub-fields.
        //
        //  [0] underlyingAsset       [1] name (dynamic)       [2] symbol (dynamic)
        //  [3] decimals              [4] baseLTVasCollateral   [5] liquidationThreshold
        //  [6] liquidationBonus      [7] reserveFactor         [8] usageAsCollateral
        //  [9] borrowingEnabled     [10] isActive             [11] isFrozen
        // [12] liquidityIndex       [13] variableBorrowIndex  [14] liquidityRate
        // [15] variableBorrowRate   [16] lastUpdateTimestamp  [17] aTokenAddress
        // [18] stableDebtToken      [19] variableDebtToken    [20] availableLiquidity
        // [21] totalScaledVariableDebt [22] priceInMarketRef  [23] priceOracle
        // [24] variableRateSlope1   [25] variableRateSlope2   [26] baseVariableBorrowRate
        // [27] optimalUsageRatio    [28] isPaused             [29] isSiloedBorrowing
        // [30] accruedToTreasury    [31] unbacked             [32] flashLoanEnabled
        // [33] debtCeiling          [34] debtCeilingDecimals  [35] borrowCap
        // [36] supplyCap            [37] eModeCategoryId      [38-39] remaining fields

        let rd = ReserveData {
            underlying_asset: read_address(data, elem_start),
            name: read_string(data, elem_start, elem_start + 32),
            symbol: read_string(data, elem_start, elem_start + 64),
            decimals: read_u256(data, elem_start + 3 * 32),
            base_ltv: read_u256(data, elem_start + 4 * 32),
            liquidation_threshold: read_u256(data, elem_start + 5 * 32),
            liquidation_bonus: read_u256(data, elem_start + 6 * 32),
            reserve_factor: read_u256(data, elem_start + 7 * 32),
            usage_as_collateral_enabled: read_bool(data, elem_start + 8 * 32),
            borrowing_enabled: read_bool(data, elem_start + 9 * 32),
            is_active: read_bool(data, elem_start + 10 * 32),
            is_frozen: read_bool(data, elem_start + 11 * 32),
            liquidity_rate: read_u128(data, elem_start + 14 * 32),
            variable_borrow_rate: read_u128(data, elem_start + 15 * 32),
            stable_borrow_rate: 0,         // removed in V3.1
            total_a_token: U256::ZERO,     // not directly in V3.1; use available_liquidity + debt
            total_stable_debt: U256::ZERO, // removed in V3.1
            total_variable_debt: read_u256(data, elem_start + 21 * 32),
            available_liquidity: read_u256(data, elem_start + 20 * 32),
            price_in_market_ref_currency: read_u256(data, elem_start + 22 * 32),
            is_paused: read_bool(data, elem_start + 28 * 32),
            supply_cap: read_u256(data, elem_start + 36 * 32),
            borrow_cap: read_u256(data, elem_start + 35 * 32),
            a_token_address: read_address(data, elem_start + 17 * 32),
        };

        reserves.push(rd);
    }

    Ok((reserves, base_currency))
}

/// Decode the ABI-encoded result of getUserReservesData.
/// Returns: (UserReserveData[], uint8)
fn decode_user_reserves_data(data: &[u8]) -> Result<Vec<UserReserveData>> {
    if data.len() < 64 {
        bail!("getUserReservesData response too short");
    }

    // Top-level: offset to array, uint8
    let array_offset: usize = read_u256(data, 0)
        .try_into()
        .map_err(|_| anyhow::anyhow!("array offset overflow"))?;
    if array_offset + 32 > data.len() {
        bail!("getUserReservesData: array offset out of bounds");
    }
    let array_len: usize = read_u256(data, array_offset)
        .try_into()
        .map_err(|_| anyhow::anyhow!("array length overflow"))?;
    if array_len > 10_000 {
        bail!("getUserReservesData: suspicious array length {}", array_len);
    }

    let mut user_reserves = Vec::with_capacity(array_len);

    // UserReserveData is a static struct (all fixed-size fields), packed directly.
    // V3.1 layout: 4 fields × 32 bytes = 128 bytes per element.
    // (stableBorrowRate, principalStableDebt, stableBorrowLastUpdateTimestamp removed)
    const FIELDS_PER_ELEMENT: usize = 4;
    const ELEMENT_SIZE: usize = FIELDS_PER_ELEMENT * 32;

    for i in 0..array_len {
        let elem_start = array_offset + 32 + i * ELEMENT_SIZE;

        // UserReserveData V3.1: 4 fields, all static
        //  0: underlyingAsset (address)
        //  1: scaledATokenBalance (uint256)
        //  2: usageAsCollateralEnabledOnUser (bool)
        //  3: scaledVariableDebt (uint256)
        let ur = UserReserveData {
            underlying_asset: read_address(data, elem_start),
            scaled_a_token_balance: read_u256(data, elem_start + 32),
            usage_as_collateral: read_bool(data, elem_start + 64),
            stable_borrow_rate: U256::ZERO,
            scaled_variable_debt: read_u256(data, elem_start + 96),
            principal_stable_debt: U256::ZERO,
            stable_borrow_last_update: U256::ZERO,
        };

        user_reserves.push(ur);
    }

    Ok(user_reserves)
}
