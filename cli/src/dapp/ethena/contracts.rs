//! Ethena sUSDe (StakedUSDeV2) contract interface.
//!
//! sUSDe is an ERC-4626 vault with a cooldown-based unstake mechanism.
//! When cooldown duration > 0, standard ERC-4626 withdraw/redeem are disabled;
//! users must call cooldownAssets() → wait 7 days → unstake().

use alloy::sol;

sol! {
    #[sol(rpc)]
    interface IStakedUSDe {
        // ERC-4626 deposit (stake USDe → receive sUSDe)
        function deposit(uint256 assets, address receiver) external returns (uint256 shares);

        // ERC-4626 read-only
        function convertToAssets(uint256 shares) external view returns (uint256 assets);
        function convertToShares(uint256 assets) external view returns (uint256 shares);
        function totalAssets() external view returns (uint256);
        function totalSupply() external view returns (uint256);
        function balanceOf(address account) external view returns (uint256);
        function asset() external view returns (address);

        // Cooldown-based unstake (replaces ERC-4626 withdraw/redeem when cooldown > 0)
        function cooldownAssets(uint256 assets) external returns (uint256 shares);
        function cooldownShares(uint256 shares) external returns (uint256 assets);
        function unstake(address receiver) external;
        function cooldownDuration() external view returns (uint24);
    }

    #[sol(rpc)]
    interface IERC20 {
        function approve(address spender, uint256 amount) external returns (bool);
        function allowance(address owner, address spender) external view returns (uint256);
        function balanceOf(address account) external view returns (uint256);
        function decimals() external view returns (uint8);
    }
}
