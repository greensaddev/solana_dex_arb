use std::{thread::sleep, time::Duration};

use solana_client::rpc_client::RpcClient;
use solana_sdk::pubkey::Pubkey;

mod common;
mod dex;
mod config;
mod arb;

use config::Config;
use crate::arb::build_arbitrage_graph;
use crate::dex::PoolMints;
use std::collections::HashMap;
use std::sync::Arc;

fn main() {
    env_logger::init();

    let cfg = Config::from_file("config.toml").expect("Failed to read config");
    println!("{:#?}", cfg);

    let rpc_url = "https://api.mainnet-beta.solana.com";
    let client = RpcClient::new(rpc_url.to_string());

    let pools_map: HashMap<Pubkey, Vec<Arc<dyn PoolMints>>>;
    // Строим HashMap пулов по mint-адресам
    match cfg.build_pools_hashmap(&client) {
        Ok(_pools_map) => {
            println!("Built pools hashmap with {} mint entries", _pools_map.len());
            pools_map = _pools_map;
        }
        Err(e) => {
            println!("Error building pools hashmap: {}", e);
            return;
        }
    }

    // Хардкодные значения для построения графа арбитража
    let start_mint: Pubkey = "So11111111111111111111111111111111111111112".parse().expect("Invalid start_mint");
    let start_amount: u64 = 1_000_000_000; // 1 SOL (9 decimals)

    // Построение графа арбитража
    match build_arbitrage_graph(&start_mint, start_amount, &pools_map, &client) {
        Ok(chains) => {
            println!("Found {} arbitrage chains", chains.len());
        }
        Err(e) => {
            eprintln!("Error building arbitrage graph: {}", e);
        }
    }

    //loop {
        /*for pool_config in &cfg.pools {
            // Обрабатываем AMM пулы
            for amm_address in &pool_config.raydium_amm {
                let pubkey: Pubkey = amm_address.parse().unwrap();
                let _ = get_pool_price(&client, &pubkey);
            }
            
            // Обрабатываем CLMM пулы
            for clmm_address in &pool_config.raydium_clmm {
                let pubkey: Pubkey = clmm_address.parse().unwrap();
                let _ = get_pool_price(&client, &pubkey);
            }
        }

        sleep(Duration::from_secs(1)); */
    //}
}
