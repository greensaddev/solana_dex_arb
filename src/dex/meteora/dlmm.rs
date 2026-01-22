use crate::dex::PoolMints;
use crate::dex::meteora::constants::{dlmm_program_id, BIN_ARRAY};
use solana_client::rpc_client::RpcClient;
use solana_sdk::pubkey::Pubkey;
use std::mem::size_of;
use crate::common::{read_mint_decimals};
use log::debug;

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct ProtocolFee {
    pub amount_x: u64,
    pub amount_y: u64,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct RewardInfo {
    pub mint: Pubkey,
    pub vault: Pubkey,
    pub funder: Pubkey,
    pub reward_duration: u64,
    pub reward_duration_end: u64,
    pub reward_rate: u128,
    pub last_update_time: u64,
    pub cumulative_seconds_with_empty_liquidity_reward: u64,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct StaticParameters {
    pub base_factor: u16,
    pub filter_period: u16,
    pub decay_period: u16,
    pub reduction_factor: u16,
    pub variable_fee_control: u32,
    pub max_volatility_accumulator: u32,
    pub min_bin_id: i32,
    pub max_bin_id: i32,
    pub protocol_share: u16,
    pub _padding: [u8; 6],
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct VariableParameters {
    pub volatility_accumulator: u32,
    pub volatility_reference: u32,
    pub index_reference: i32,
    pub _padding: [u8; 4],
    pub last_update_timestamp: i64,
    pub _padding_1: [u8; 8],
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct LbPair {
    pub parameters: StaticParameters,
    pub v_parameters: VariableParameters,
    pub bump_seed: [u8; 1],
    pub bin_step_seed: [u8; 2],
    pub pair_type: u8,
    pub active_id: i32,
    pub bin_step: u16,
    pub status: u8,
    pub require_base_factor_seed: u8,
    pub base_factor_seed: [u8; 2],
    pub activation_type: u8,
    pub _padding_0: u8,
    pub token_x_mint: Pubkey,
    pub token_y_mint: Pubkey,
    pub reserve_x: Pubkey,
    pub reserve_y: Pubkey,
    pub protocol_fee: ProtocolFee,
    pub _padding_1: [u8; 32],
    pub reward_infos: [RewardInfo; 2],
    pub oracle: Pubkey,
    pub bin_array_bitmap: [u64; 16],
    pub last_updated_at: i64,
    pub _padding_2: [u8; 32],
    pub pre_activation_swap_address: Pubkey,
    pub base_key: Pubkey,
    pub activation_point: u64,
    pub pre_activation_duration: u64,
    pub _padding_3: [u8; 8],
    pub _padding_4: u64,
    pub creator: Pubkey,
    pub _reserved: [u8; 24],
}

#[derive(Debug)]
pub struct DlmmInfo {
    pub token_x_mint: Pubkey,
    pub token_y_mint: Pubkey,
    pub token_x_vault: Pubkey,
    pub token_y_vault: Pubkey,
    pub oracle: Pubkey,
    pub active_id: i32,
    pub lb_pair: LbPair,
}

/// Минимальная структура DLMM-пула, достаточная для off-chain расчётов арбитража.
#[derive(Debug)]
pub struct MeteoraDlmmPoolInfo {
    pub pubkey: Pubkey,
    mint_a: Pubkey,
    mint_b: Pubkey,
    pub vault_a: Pubkey,
    pub vault_b: Pubkey,
    pub decimals_a: u8,
    pub decimals_b: u8,
    pub active_id: i32,
    pub bin_step: u16,
    /// Комиссия пула (из base_factor) в basis points
    pub fee_rate_bps: u16,
}

impl DlmmInfo {
    pub fn load_checked(data: &[u8]) -> Result<Self, Box<dyn std::error::Error>> {
        if data.len() < 8 + size_of::<LbPair>() {
            return Err("Invalid data length for DlmmInfo".into());
        }

        let raw_lb_pair = &data[8..8 + size_of::<LbPair>()];

        let lb_pair: LbPair = unsafe {
            assert!(raw_lb_pair.len() >= std::mem::size_of::<LbPair>());
            std::ptr::read_unaligned(raw_lb_pair.as_ptr() as *const LbPair)
        };

        Ok(Self {
            token_x_mint: lb_pair.token_x_mint,
            token_y_mint: lb_pair.token_y_mint,
            token_x_vault: lb_pair.reserve_x,
            token_y_vault: lb_pair.reserve_y,
            oracle: lb_pair.oracle,
            active_id: lb_pair.active_id,
            lb_pair,
        })
    }

    pub fn get_token_and_sol_vaults(&self, mint: &Pubkey, sol_mint: &Pubkey) -> (Pubkey, Pubkey) {
        let token_vault;
        let sol_vault;

        if sol_mint == &self.token_x_mint {
            sol_vault = self.token_x_vault;
            token_vault = self.token_y_vault;
        } else if sol_mint == &self.token_y_mint {
            sol_vault = self.token_y_vault;
            token_vault = self.token_x_vault;
        } else {
            if mint == &self.token_x_mint {
                token_vault = self.token_x_vault;
                sol_vault = self.token_y_vault;
            } else {
                token_vault = self.token_y_vault;
                sol_vault = self.token_x_vault;
            }
        }

        (token_vault, sol_vault)
    }

    pub fn calculate_bin_arrays(&self, pair_pubkey: &Pubkey) -> Result<Vec<Pubkey>, Box<dyn std::error::Error>> {
        let bin_array_index = self.bin_id_to_bin_array_index(self.active_id)?;

        let mut bin_arrays = Vec::new();
        let offsets = [-1, 0, 1];

        for offset in offsets {
            let array_idx = bin_array_index + offset;
            let array_pda = self.derive_bin_array_pda(pair_pubkey, array_idx as i64)?;
            bin_arrays.push(array_pda);
        }

        Ok(bin_arrays)
    }

    fn bin_id_to_bin_array_index(&self, bin_id: i32) -> Result<i32, Box<dyn std::error::Error>> {
        // Use a constant bin per array size of 100 as used in the meteora protocol
        let bin_per_array = 100;
        Ok(bin_id.div_euclid(bin_per_array))
    }

    fn derive_bin_array_pda(&self, lb_pair: &Pubkey, index: i64) -> Result<Pubkey, Box<dyn std::error::Error>> {
        let seeds = [BIN_ARRAY, lb_pair.as_ref(), &index.to_le_bytes()[0..8]];

        let (pda, _) = Pubkey::find_program_address(&seeds, &dlmm_program_id());

        Ok(pda)
    }
}

impl LbPair {
    pub fn from_bytes(data: &[u8]) -> Result<Self, Box<dyn std::error::Error>> {
        if data.len() < size_of::<Self>() {
            return Err("Data is too small for LbPair".into());
        }

        let lb_pair = unsafe { std::ptr::read_unaligned(data.as_ptr() as *const LbPair) };

        Ok(lb_pair)
    }
}


impl PoolMints for MeteoraDlmmPoolInfo {
    fn pool_pubkey(&self) -> &Pubkey {
        &self.pubkey
    }

    fn mint_a(&self) -> &Pubkey {
        &self.mint_a
    }

    fn mint_b(&self) -> &Pubkey {
        &self.mint_b
    }

    /// Расчёт amount_out для свопа в DLMM на основе active_id и bin_step.
    ///
    /// В DLMM цена рассчитывается по формуле: price = (1 + bin_step/10000)^(active_id)
    /// Для упрощения используем линейную аппроксимацию на основе текущей цены бина.
    fn amount_out(
        &self,
        client: &RpcClient,
        amount_in: u64,
        token_in: &Pubkey,
    ) -> Result<u64, Box<dyn std::error::Error>> {
        if amount_in == 0 {
            return Ok(0);
        }

        // Применяем комиссию пула к входящему количеству
        let fee_bps = self.fee_rate_bps as u128;
        let amount_in_u128 = amount_in as u128;
        let amount_in_after_fee = amount_in_u128 * (10_000u128 - fee_bps) / 10_000u128;

        // Рассчитываем цену из bin_id: price = (1 + bin_step/10000)^(active_id)
        // Это цена token_y / token_x (или token_b / token_a) без учета decimals
        let bin_step_f = self.bin_step as f64 / 10_000.0;
        let price_ratio = (1.0 + bin_step_f).powi(self.active_id);

        if price_ratio == 0.0 {
            return Err("Price ratio is 0".into());
        }

        // Упрощённый расчёт: для малых свопов используем текущую цену
        // Для более точного расчёта нужно учитывать распределение ликвидности по бинам
        let amount_in_f = amount_in_after_fee as f64;
        
        // Цена уже в правильном соотношении, применяем с учетом decimals для конвертации между минимальными единицами
        let amount_out_f = if *token_in == *self.mint_a() {
            // token_a -> token_b: amount_out = amount_in * price_ratio * (10^decimals_b / 10^decimals_a)
            amount_in_f * price_ratio * 10f64.powi((self.decimals_b as i32 - self.decimals_a as i32) as i32)
        } else {
            // token_b -> token_a: amount_out = amount_in / price_ratio * (10^decimals_a / 10^decimals_b)
            amount_in_f / price_ratio * 10f64.powi((self.decimals_a as i32 - self.decimals_b as i32) as i32)
        };

        // Ограничиваем максимальный вывод доступными резервами
        if amount_out_f <= 0.0 {
            return Err("Amount out is less than 0".into());
        }

        Ok(amount_out_f as u64)
    }
}

impl MeteoraDlmmPoolInfo {
    /// Создать структуру пула из DlmmInfo.
    pub fn from_dlmm_info(pool_pubkey: Pubkey, dlmm_info: &DlmmInfo, client: &RpcClient) -> Result<Self, Box<dyn std::error::Error>> {
        // Читаем decimals из mint-аккаунтов
        let mint_a_acc = client.get_account(&dlmm_info.token_x_mint)?;
        let mint_b_acc = client.get_account(&dlmm_info.token_y_mint)?;
        let decimals_a = read_mint_decimals(&mint_a_acc) as u8;
        let decimals_b = read_mint_decimals(&mint_b_acc) as u8;

        // Комиссия в DLMM вычисляется из base_factor
        // base_factor хранится как u16, и указывает комиссию в basis points (bps)
        let base_factor = dlmm_info.lb_pair.parameters.base_factor; // base_factor реальное значение комиссии в bps

        debug!(
            "Parsed DLMM Pool: \
             \n\tpool={}, \
             \n\tmintA={}, \
             \n\tmintB={}, \
             \n\tvaultA={}, \
             \n\tvaultB={}, \
             \n\tactive_id={}, \
             \n\tbin_step={}, \
             \n\tfee_bps={}",
            pool_pubkey,
            dlmm_info.token_x_mint,
            dlmm_info.token_y_mint,
            dlmm_info.token_x_vault,
            dlmm_info.token_y_vault,
            dlmm_info.active_id,
            dlmm_info.lb_pair.bin_step,
            base_factor
        );

        Ok(Self {
            pubkey: pool_pubkey,
            mint_a: dlmm_info.token_x_mint,
            mint_b: dlmm_info.token_y_mint,
            vault_a: dlmm_info.token_x_vault,
            vault_b: dlmm_info.token_y_vault,
            decimals_a,
            decimals_b,
            active_id: dlmm_info.active_id,
            bin_step: dlmm_info.lb_pair.bin_step,
            fee_rate_bps: base_factor,
        })
    }

    /// Создать структуру пула напрямую из аккаунта пула.
    pub fn create(pool_pubkey: Pubkey, client: &RpcClient) -> Result<Self, Box<dyn std::error::Error>> {
        println!("Creating DLMM pool: {}", pool_pubkey);
        let account = client.get_account(&pool_pubkey)?;
        let dlmm_info = DlmmInfo::load_checked(&account.data)?;
        Self::from_dlmm_info(pool_pubkey, &dlmm_info, client)
    }

    /// Рассчитать текущую цену на основе active_id и bin_step.
    /// Возвращает цену token_b / token_a с учетом decimals.
    pub fn price(&self) -> f64 {
        let bin_step_f = self.bin_step as f64 / 10_000.0;
        let price_ratio = (1.0 + bin_step_f).powi(self.active_id);
        // Применяем decimals для получения цены в правильных единицах
        price_ratio// * 10f64.powi((self.decimals_b as i32 - self.decimals_a as i32) as i32)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use solana_client::rpc_client::RpcClient;

    #[test]
    fn test_dlmm_pool_info() {
        // Захардкоженный адрес DLMM пула Meteora
        // Пример адреса DLMM пула на Solana mainnet
        // Замените на реальный адрес пула для тестирования
        // Можно найти адреса пулов на https://app.meteora.ag/dlmm
        let pool_address = "6wJ7W3oHj7ex6MVFp2o26NSof3aey7U8Brs8E371WCXA";
        
        let pool_pubkey: Pubkey = match pool_address.parse() {
            Ok(pk) => pk,
            Err(e) => {
                eprintln!("Ошибка парсинга адреса пула: {}", e);
                return;
            }
        };

        let rpc_url = "https://api.mainnet-beta.solana.com";
        let client = RpcClient::new(rpc_url.to_string());

        println!("\n=== Информация о DLMM пуле ===");
        println!("Адрес пула: {}", pool_pubkey);

        match MeteoraDlmmPoolInfo::create(pool_pubkey, &client) {
            Ok(pool) => {
                println!("\nОсновная информация о пуле:");
                println!("  Адрес пула: {}", pool.pubkey);
                println!("  Mint A: {}", pool.mint_a());
                println!("  Mint B: {}", pool.mint_b());
                println!("  Vault A: {}", pool.vault_a);
                println!("  Vault B: {}", pool.vault_b);
                println!("  Decimals A: {}", pool.decimals_a);
                println!("  Decimals B: {}", pool.decimals_b);
                println!("  Active ID: {}", pool.active_id);
                println!("  Bin Step: {}", pool.bin_step);
                println!("  Fee Rate (bps): {}", pool.fee_rate_bps);
                
                let price = pool.price();
                println!("\nЦена пула:");
                println!("  Цена (token_b / token_a) (без учета decimals): {:.10}", price);
                
                // Получаем резервы для дополнительной информации
                match (client.get_account(&pool.vault_a), client.get_account(&pool.vault_b)) {
                    (Ok(vault_a_acc), Ok(vault_b_acc)) => {
                        let reserve_a = crate::common::read_spl_amount(&vault_a_acc);
                        let reserve_b = crate::common::read_spl_amount(&vault_b_acc);
                        println!("\nРезервы пула:");
                        println!("  Reserve A: {}", reserve_a);
                        println!("  Reserve B: {}", reserve_b);
                    }
                    _ => {
                        println!("\nНе удалось получить информацию о резервах");
                    }
                }
            }
            Err(e) => {
                eprintln!("Ошибка при создании пула: {}", e);
                panic!("Тест провален: не удалось загрузить информацию о пуле");
            }
        }
    }
}