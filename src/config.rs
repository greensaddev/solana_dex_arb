use serde::Deserialize;
use std::{path::Path, collections::HashMap, sync::Arc};
use solana_client::rpc_client::RpcClient;
use solana_sdk::pubkey::Pubkey;

use crate::dex::PoolMints;
use crate::dex::raydium::amm::RaydiumAmmPoolInfo;
use crate::dex::raydium::clmm::RaydiumClmmPoolInfo;

#[derive(Debug, Deserialize)]
pub struct PoolConfig {
    pub mint: String,
    #[serde(default)]
    pub raydium_amm: Vec<String>,
    #[serde(default)]
    pub raydium_clmm: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct Config {
    pub pools: Vec<PoolConfig>,
}

impl Config {
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self, Box<dyn std::error::Error>> {
        let content = std::fs::read_to_string(path)?;
        let cfg: Config = toml::from_str(&content)?;
        Ok(cfg)
    }

    /// Строит HashMap, где ключ - mint адрес, значение - вектор указателей на объекты трейта PoolMints
    /// 
    /// Структура конфига: для каждого mint указываются списки пулов разных типов (raydium_amm, raydium_clmm)
    pub fn build_pools_hashmap(
        &self,
        client: &RpcClient,
    ) -> Result<HashMap<Pubkey, Vec<Arc<dyn PoolMints>>>, Box<dyn std::error::Error>> {
        let mut pools_map: HashMap<Pubkey, Vec<Arc<dyn PoolMints>>> = HashMap::new();

        for pool_config in &self.pools {
            let mint_key: Pubkey = pool_config.mint.parse()?;
            let mut pools_for_mint: Vec<Arc<dyn PoolMints>> = Vec::new();

            // Создаем AMM пулы
            for amm_address in &pool_config.raydium_amm {
                let pool_pubkey: Pubkey = amm_address.parse()?;
                let amm_pool = RaydiumAmmPoolInfo::create(pool_pubkey, client)?;
                pools_for_mint.push(Arc::new(amm_pool));
            }

            // Создаем CLMM пулы
            for clmm_address in &pool_config.raydium_clmm {
                let pool_pubkey: Pubkey = clmm_address.parse()?;
                let clmm_pool = RaydiumClmmPoolInfo::create(pool_pubkey, client)?;
                pools_for_mint.push(Arc::new(clmm_pool));
            }

            // Добавляем все пулы для данного mint в HashMap
            if !pools_for_mint.is_empty() {
                pools_map.insert(mint_key, pools_for_mint);
            }
        }

        Ok(pools_map)
    }
}
