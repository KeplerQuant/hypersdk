use alloy::sol;

sol! {
    type Id is bytes32;

    #[derive(Debug)]
    struct Market {
        uint128 totalSupplyAssets;
        uint128 totalSupplyShares;
        uint128 totalBorrowAssets;
        uint128 totalBorrowShares;
        uint128 lastUpdate;
        uint128 fee;
    }

    #[derive(Debug)]
    struct MarketParams {
        address loanToken;
        address collateralToken;
        address oracle;
        address irm;
        uint256 lltv;
    }

    #[derive(Debug)]
    struct MarketConfig {
        uint184 cap;
        bool enabled;
        uint64 removableAt;
    }

    #[sol(rpc)]
    contract Morpho {
        // ========== events ============
        event CreateMarket(Id indexed id, MarketParams marketParams);
        event Supply(Id indexed id, address indexed caller, address indexed onBehalf, uint256 assets, uint256 shares);
        event Withdraw(
            Id indexed id,
            address caller,
            address indexed onBehalf,
            address indexed receiver,
            uint256 assets,
            uint256 shares
        );
        event Borrow(
            Id indexed id,
            address caller,
            address indexed onBehalf,
            address indexed receiver,
            uint256 assets,
            uint256 shares
        );
        event Repay(Id indexed id, address indexed caller, address indexed onBehalf, uint256 assets, uint256 shares);
        event SupplyCollateral(Id indexed id, address indexed caller, address indexed onBehalf, uint256 assets);
        event Liquidate(
            Id indexed id,
            address indexed caller,
            address indexed borrower,
            uint256 repaidAssets,
            uint256 repaidShares,
            uint256 seizedAssets,
            uint256 badDebtAssets,
            uint256 badDebtShares
        );
        event FlashLoan(address indexed caller, address indexed token, uint256 assets);
        event AccrueInterest(Id indexed id, uint256 prevBorrowRate, uint256 interest, uint256 feeShares);

        // ========= functions =========
        function owner() external view returns (address);
        function feeRecipient() external view returns (address);
        function isIrmEnabled(address irm) external view returns (bool);
        function isAuthorized(address authorizer, address authorized) external view returns (bool);
        function nonce(address authorizer) external view returns (uint256);
        function market(Id market) returns (Market);
        function idToMarketParams(Id market) returns (MarketParams);
        function convertToAssets(uint256 shares) external view returns (uint256 assets);
        function position(bytes32 id, address user)
            external
            view
            returns (uint256 supplyShares, uint128 borrowShares, uint128 collateral);
        function supply(
                MarketParams memory marketParams,
                uint256 assets,
                uint256 shares,
                address onBehalf,
                bytes memory data
            ) external returns (uint256 assetsSupplied, uint256 sharesSupplied);
        function withdraw(
                MarketParams memory marketParams,
                uint256 assets,
                uint256 shares,
                address onBehalf,
                address receiver
            ) external returns (uint256 assetsWithdrawn, uint256 sharesWithdrawn);
        function borrow(
                MarketParams memory marketParams,
                uint256 assets,
                uint256 shares,
                address onBehalf,
                address receiver
            ) external returns (uint256 assetsBorrowed, uint256 sharesBorrowed);
        function repay(
                MarketParams memory marketParams,
                uint256 assets,
                uint256 shares,
                address onBehalf,
                bytes memory data
            ) external returns (uint256 assetsRepaid, uint256 sharesRepaid);

    }

    #[sol(rpc)]
    contract MetaMorpho {
        bytes32[] public supplyQueue;

        function MORPHO() external view returns (address);
        function fee() returns (uint96);
        function supplyQueueLength() external view returns (uint256);
        function config(Id market) returns (MarketConfig);
    }

    #[sol(rpc)]
    contract AdaptativeCurveIrm {
        function MORPHO() external view returns (address);
        function borrowRateView(MarketParams memory marketParams, Market memory market) external returns (uint256);
    }
}
