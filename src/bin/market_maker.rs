#![warn(clippy::all, clippy::nursery, clippy::pedantic)]

use ethers::signers::LocalWallet;
use hyperliquid_rust_sdk::{Input, MarketMaker};
use std::sync::Arc;
use tokio::sync::Mutex;

#[tokio::main]
async fn main() {
    env_logger::init();

    // Key was randomly generated for testing and shouldn't be used with any real funds
    let wallet: LocalWallet = "e908f86dbb4d55ac876378565aafeabc187f6690f046459397b17d9b9a19688e"
        .parse()
        .unwrap();

    // Define a vector of market maker configurations
    let market_makers = vec![
        // SOL Market Maker
        Input {
            asset: "SOL".to_string(),
            target_liquidity: 0.1,
            max_bps_diff: 10,
            half_spread: 5,
            max_absolute_position_size: 2.0,
            decimals: 2,
            wallet: wallet.clone(),
        },
        // ETH Market Maker
        Input {
            asset: "ETH".to_string(),
            target_liquidity: 0.003,
            max_bps_diff: 10,
            half_spread: 5,
            max_absolute_position_size: 0.06,
            decimals: 1,
            wallet: wallet.clone(),
        },
        // BTC Market Maker
        Input {
            asset: "BTC".to_string(),
            target_liquidity: 0.0002,
            max_bps_diff: 10,
            half_spread: 5,
            max_absolute_position_size: 0.004,
            decimals: 0,
            wallet: wallet.clone(),
        },
        // ARB Market Maker
        Input {
            asset: "ARB".to_string(),
            target_liquidity: 12.0,
            max_bps_diff: 10,
            half_spread: 5,
            max_absolute_position_size: 240.0,
            decimals: 4,
            wallet: wallet.clone(),
        },
        // kPEPE Market Maker
        Input {
            asset: "kPEPE".to_string(),
            target_liquidity: 1100.0,
            max_bps_diff: 16,
            half_spread: 8,
            max_absolute_position_size: 20000.0,
            decimals: 5,
            wallet: wallet.clone(),
        },
        // RNDR Market Maker
        Input {
            asset: "RNDR".to_string(),
            target_liquidity: 1.5,
            max_bps_diff: 10,
            half_spread: 5,
            max_absolute_position_size: 30.0,
            decimals: 3,
            wallet,
        },
    ];

    // Create and start each market maker in a separate task
    let tasks = market_makers
        .into_iter()
        .map(|input| {
            let wallet = Arc::new(Mutex::new(input.wallet));
            tokio::spawn(async move {
                MarketMaker::new(Input {
                    wallet: wallet.lock().await.clone(),
                    ..input
                })
                .await
                .expect("Failed to create MarketMaker")
                .start()
                .await;
            })
        })
        .collect::<Vec<_>>();

    // Wait for all tasks to complete
    for task in tasks {
        task.await.unwrap();
    }
}
