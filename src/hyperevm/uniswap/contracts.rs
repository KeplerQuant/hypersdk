use alloy::sol;

sol! {
    #[derive(Debug)]
    struct PositionData {
        uint96 nonce;
        address operator;
        address token0;
        address token1;
        uint24 fee;
        int24 tickLower;
        int24 tickUpper;
        uint128 liquidity;
        uint256 feeGrowthInside0LastX128;
        uint256 feeGrowthInside1LastX128;
        uint128 tokensOwed0;
        uint128 tokensOwed1;
    }

    #[derive(Debug)]
    struct CollectParams {
        uint256 tokenId;
        address recipient;
        uint128 amount0Max;
        uint128 amount1Max;
    }

    struct IncreaseLiquidityParams {
        uint256 tokenId;
        uint256 amount0Desired;
        uint256 amount1Desired;
        uint256 amount0Min;
        uint256 amount1Min;
        uint256 deadline;
    }

    #[derive(Debug)]
    struct DecreaseLiquidityParams {
        uint256 tokenId;
        uint128 liquidity;
        uint256 amount0Min;
        uint256 amount1Min;
        uint256 deadline;
    }

    struct MintParams {
        address token0;
        address token1;
        uint24 fee;
        int24 tickLower;
        int24 tickUpper;
        uint256 amount0Desired;
        uint256 amount1Desired;
        uint256 amount0Min;
        uint256 amount1Min;
        address recipient;
        uint256 deadline;
    }

    /// See more:
    ///
    /// <https://github.com/Uniswap/v3-periphery/blob/0682387198a24c7cd63566a2c58398533860a5d1/contracts/interfaces/INonfungiblePositionManager.sol#L110>
    #[sol(rpc)]
    contract NFTPositionManager {
        function balanceOf(address owner) external view returns (uint256);
        function tokenOfOwnerByIndex(address owner, uint256 index) external view returns (uint256 tokenId);
        function positions(uint256 tokenId) external view returns (PositionData);
        function mint(MintParams calldata params)
            external
            payable
            returns (
                uint256 tokenId,
                uint128 liquidity,
                uint256 amount0,
                uint256 amount1
            );
        function increaseLiquidity(IncreaseLiquidityParams calldata params)
            external
            payable
            override
            checkDeadline(params.deadline)
            returns (
                uint128 liquidity,
                uint256 amount0,
                uint256 amount1
            );
        function decreaseLiquidity(DecreaseLiquidityParams params)
            external
            returns (uint128 liquidity, uint256 amount0, uint256 amount1);

        function collect(CollectParams params)
            external
            returns (uint256 amount0, uint256 amount1);

        function burn(uint256 tokenId) external payable;

        event IncreaseLiquidity(uint256 indexed tokenId, uint128 liquidity, uint256 amount0, uint256 amount1);
        event DecreaseLiquidity(uint256 indexed tokenId, uint128 liquidity, uint256 amount0, uint256 amount1);
        event Collect(uint256 indexed tokenId, address recipient, uint256 amount0, uint256 amount1);
    }

    /// See more:
    ///
    /// <https://github.com/Uniswap/v3-core/blob/main/contracts/interfaces/pool/IUniswapV3PoolActions.sol>
    /// <https://github.com/Uniswap/v3-core/blob/main/contracts/interfaces/pool/IUniswapV3PoolEvents.sol>
    /// <https://github.com/Uniswap/v3-core/blob/main/contracts/interfaces/pool/IUniswapV3PoolImmutables.sol>
    /// <https://github.com/Uniswap/v3-core/blob/main/contracts/interfaces/pool/IUniswapV3PoolOwnerActions.sol>
    /// <https://github.com/Uniswap/v3-core/blob/main/contracts/interfaces/pool/IUniswapV3PoolState.sol>
    #[sol(rpc)]
    contract Pool {
        function slot0() external view returns (
            uint160 sqrtPriceX96,
            int24 tick,
            uint16 observationIndex,
            uint16 observationCardinality,
            uint16 observationCardinalityNext,
            uint8 feeProtocol,
            bool unlocked
        );
        // https://github.com/Uniswap/v3-core/blob/d8b1c635c275d2a9450bd6a78f3fa2484fef73eb/contracts/interfaces/pool/IUniswapV3PoolActions.sol#L59
        function burn(
            int24 tickLower,
            int24 tickUpper,
            uint128 amount
        ) external returns (uint256 amount0, uint256 amount1);
        // https://github.com/Uniswap/v3-core/blob/d8b1c635c275d2a9450bd6a78f3fa2484fef73eb/contracts/interfaces/pool/IUniswapV3PoolActions.sol#L75
        function swap(
            address recipient,
            bool zeroForOne,
            int256 amountSpecified,
            uint160 sqrtPriceLimitX96,
            bytes calldata data
        ) external returns (int256 amount0, int256 amount1);
        // https://github.com/Uniswap/v3-core/blob/d8b1c635c275d2a9450bd6a78f3fa2484fef73eb/contracts/interfaces/pool/IUniswapV3PoolActions.sol#L91
        function flash(
            address recipient,
            uint256 amount0,
            uint256 amount1,
            bytes calldata data
        ) external;
        // https://github.com/Uniswap/v3-core/blob/d8b1c635c275d2a9450bd6a78f3fa2484fef73eb/contracts/interfaces/pool/IUniswapV3PoolActions.sol#L23
        function mint(
            address recipient,
            int24 tickLower,
            int24 tickUpper,
            uint128 amount,
            bytes calldata data
        ) external returns (uint256 amount0, uint256 amount1);
        // https://github.com/Uniswap/v3-core/blob/d8b1c635c275d2a9450bd6a78f3fa2484fef73eb/contracts/interfaces/pool/IUniswapV3PoolActions.sol#L43
        function collect(
            address recipient,
            int24 tickLower,
            int24 tickUpper,
            uint128 amount0Requested,
            uint128 amount1Requested
        ) external returns (uint128 amount0, uint128 amount1);
        // https://github.com/Uniswap/v3-core/blob/d8b1c635c275d2a9450bd6a78f3fa2484fef73eb/contracts/interfaces/pool/IUniswapV3PoolImmutables.sol#L9
        function factory() external view returns (address);
        // https://github.com/Uniswap/v3-core/blob/d8b1c635c275d2a9450bd6a78f3fa2484fef73eb/contracts/interfaces/pool/IUniswapV3PoolImmutables.sol#L13
        function token0() external view returns (address);
        // https://github.com/Uniswap/v3-core/blob/d8b1c635c275d2a9450bd6a78f3fa2484fef73eb/contracts/interfaces/pool/IUniswapV3PoolImmutables.sol#L17
        function token1() external view returns (address);
        // https://github.com/Uniswap/v3-core/blob/d8b1c635c275d2a9450bd6a78f3fa2484fef73eb/contracts/interfaces/pool/IUniswapV3PoolImmutables.sol#L21
        function fee() external view returns (uint24);
        // https://github.com/Uniswap/v3-core/blob/d8b1c635c275d2a9450bd6a78f3fa2484fef73eb/contracts/interfaces/pool/IUniswapV3PoolImmutables.sol#L28
        function tickSpacing() external view returns (int24);
        // https://github.com/Uniswap/v3-core/blob/d8b1c635c275d2a9450bd6a78f3fa2484fef73eb/contracts/interfaces/pool/IUniswapV3PoolImmutables.sol#L34
        function maxLiquidityPerTick() external view returns (uint128);

        event Mint(
            address sender,
            address indexed owner,
            int24 indexed tickLower,
            int24 indexed tickUpper,
            uint128 amount,
            uint256 amount0,
            uint256 amount1
        );
        event Collect(
            address indexed owner,
            address recipient,
            int24 indexed tickLower,
            int24 indexed tickUpper,
            uint128 amount0,
            uint128 amount1
        );
        event Swap(
            address indexed sender,
            address indexed recipient,
            int256 amount0,
            int256 amount1,
            uint160 sqrtPriceX96,
            uint128 liquidity,
            int24 tick
        );
    }

    /// See more:
    ///
    /// <https://github.com/Uniswap/v3-periphery/blob/0682387198a24c7cd63566a2c58398533860a5d1/contracts/interfaces/ISwapRouter.sol>
    #[sol(rpc)]
    contract SwapRouter {
        // https://github.com/Uniswap/v3-periphery/blob/0682387198a24c7cd63566a2c58398533860a5d1/contracts/interfaces/ISwapRouter.sol#L10
        #[derive(Debug)]
        struct ExactInputSingleParams {
            address tokenIn;
            address tokenOut;
            uint24 fee;
            address recipient;
            uint256 deadline;
            uint256 amountIn;
            uint256 amountOutMinimum;
            uint160 sqrtPriceLimitX96;
        }
        // https://github.com/Uniswap/v3-periphery/blob/0682387198a24c7cd63566a2c58398533860a5d1/contracts/interfaces/ISwapRouter.sol#L24
        function exactInputSingle(ExactInputSingleParams calldata params) external payable returns (uint256 amountOut);
        // https://github.com/Uniswap/v3-periphery/blob/0682387198a24c7cd63566a2c58398533860a5d1/contracts/interfaces/ISwapRouter.sol#L26
        struct ExactInputParams {
            bytes path;
            address recipient;
            uint256 deadline;
            uint256 amountIn;
            uint256 amountOutMinimum;
        }
        // https://github.com/Uniswap/v3-periphery/blob/0682387198a24c7cd63566a2c58398533860a5d1/contracts/interfaces/ISwapRouter.sol#L37
        function exactInput(ExactInputParams calldata params) external payable returns (uint256 amountOut);
        // https://github.com/Uniswap/v3-periphery/blob/0682387198a24c7cd63566a2c58398533860a5d1/contracts/interfaces/ISwapRouter.sol#L39
        struct ExactOutputSingleParams {
            address tokenIn;
            address tokenOut;
            uint24 fee;
            address recipient;
            uint256 deadline;
            uint256 amountOut;
            uint256 amountInMaximum;
            uint160 sqrtPriceLimitX96;
        }
        // https://github.com/Uniswap/v3-periphery/blob/0682387198a24c7cd63566a2c58398533860a5d1/contracts/interfaces/ISwapRouter.sol#L53
        function exactOutputSingle(ExactOutputSingleParams calldata params) external payable returns (uint256 amountIn);
        // https://github.com/Uniswap/v3-periphery/blob/0682387198a24c7cd63566a2c58398533860a5d1/contracts/interfaces/ISwapRouter.sol#L55
        struct ExactOutputParams {
            bytes path;
            address recipient;
            uint256 deadline;
            uint256 amountOut;
            uint256 amountInMaximum;
        }
        // https://github.com/Uniswap/v3-periphery/blob/0682387198a24c7cd63566a2c58398533860a5d1/contracts/interfaces/ISwapRouter.sol#L66
        function exactOutput(ExactOutputParams calldata params) external payable returns (uint256 amountIn);
    }

    /// See more:
    ///
    /// <https://github.com/Uniswap/v3-core/blob/main/contracts/interfaces/IUniswapV3Factory.sol>
    #[sol(rpc)]
    contract Factory {
        function getPool(address token0, address token1, uint24 fee)
            external
            view
            returns (address pool);

        event PoolCreated(
            address indexed token0,
            address indexed token1,
            uint24 indexed fee,
            int24 tickSpacing,
            address pool
        );
    }

    /// See more:
    ///
    /// <https://github.com/Uniswap/v3-periphery/blob/0682387198a24c7cd63566a2c58398533860a5d1/contracts/interfaces/IQuoterV2.sol>
    #[sol(rpc)]
    contract QuoterV2 {
        function quoteExactInput(bytes memory path, uint256 amountIn)
            external
            returns (
                uint256 amountOut,
                uint160[] memory sqrtPriceX96AfterList,
                uint32[] memory initializedTicksCrossedList,
                uint256 gasEstimate
            );

        struct QuoteExactInputSingleParams {
            address tokenIn;
            address tokenOut;
            uint256 amountIn;
            uint24 fee;
            uint160 sqrtPriceLimitX96;
        }

        function quoteExactInputSingle(QuoteExactInputSingleParams memory params)
            external
            returns (
                uint256 amountOut,
                uint160 sqrtPriceX96After,
                uint32 initializedTicksCrossed,
                uint256 gasEstimate
            );

        function quoteExactOutput(bytes memory path, uint256 amountOut)
            external
            returns (
                uint256 amountIn,
                uint160[] memory sqrtPriceX96AfterList,
                uint32[] memory initializedTicksCrossedList,
                uint256 gasEstimate
            );

        struct QuoteExactOutputSingleParams {
            address tokenIn;
            address tokenOut;
            uint256 amount;
            uint24 fee;
            uint160 sqrtPriceLimitX96;
        }

        function quoteExactOutputSingle(QuoteExactOutputSingleParams memory params)
            external
            returns (
                uint256 amountIn,
                uint160 sqrtPriceX96After,
                uint32 initializedTicksCrossed,
                uint256 gasEstimate
            );
    }
}
