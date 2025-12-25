use solana_client::rpc_client::RpcClient;
use solana_sdk::pubkey::Pubkey;
use std::convert::TryInto;
use log::{info, debug};

use crate::common::read_mint_decimals;
use crate::dex::PoolMints;

#[derive(Default, Debug)]
pub struct AmmConfig {
    /// Bump to identify PDA
    pub bump: u8,
    pub index: u16,
    /// Address of the protocol owner
    pub owner: Pubkey,
    /// The protocol fee
    pub protocol_fee_rate: u32,
    /// The trade fee, denominated in hundredths of a bip (10^-6)
    pub trade_fee_rate: u32,
    /// The tick spacing
    pub tick_spacing: u16,
    /// The fund fee, denominated in hundredths of a bip (10^-6)
    pub fund_fee_rate: u32,
    // padding space for upgrade
    pub padding_u32: u32,
    pub fund_owner: Pubkey,
    pub padding: [u64; 3],
}

#[derive(Default, Debug, PartialEq, Eq)]
pub struct RewardInfo {
    /// Reward state
    pub reward_state: u8,
    /// Reward open time
    pub open_time: u64,
    /// Reward end time
    pub end_time: u64,
    /// Reward last update time
    pub last_update_time: u64,
    /// Q64.64 number indicates how many tokens per second are earned per unit of liquidity.
    pub emissions_per_second_x64: u128,
    /// The total amount of reward emissioned
    pub reward_total_emissioned: u64,
    /// The total amount of claimed reward
    pub reward_claimed: u64,
    /// Reward token mint.
    pub token_mint: Pubkey,
    /// Reward vault token account.
    pub token_vault: Pubkey,
    /// The owner that has permission to set reward param
    pub authority: Pubkey,
    /// Q64.64 number that tracks the total tokens earned per unit of liquidity since the reward
    /// emissions were turned on.
    pub reward_growth_global_x64: u128,
}

const REWARD_NUM: usize = 3;

#[derive(Default, Debug)]
pub struct PoolState {
    /// Bump to identify PDA
    pub bump: [u8; 1],
    // Which config the pool belongs
    pub amm_config: Pubkey,
    // Pool creator
    pub owner: Pubkey,

    /// Token pair of the pool, where token_mint_0 address < token_mint_1 address
    pub token_mint_0: Pubkey,
    pub token_mint_1: Pubkey,

    /// Token pair vault
    pub token_vault_0: Pubkey,
    pub token_vault_1: Pubkey,

    /// observation account key
    pub observation_key: Pubkey,

    /// mint0 and mint1 decimals
    pub mint_decimals_0: u8,
    pub mint_decimals_1: u8,

    /// The minimum number of ticks between initialized ticks
    pub tick_spacing: u16,
    /// The currently in range liquidity available to the pool.
    pub liquidity: u128,
    /// The current price of the pool as a sqrt(token_1/token_0) Q64.64 value
    pub sqrt_price_x64: u128,
    /// The current tick of the pool, i.e. according to the last tick transition that was run.
    pub tick_current: i32,

    pub padding3: u16,
    pub padding4: u16,

    /// The fee growth as a Q64.64 number, i.e. fees of token_0 and token_1 collected per
    /// unit of liquidity for the entire life of the pool.
    pub fee_growth_global_0_x64: u128,
    pub fee_growth_global_1_x64: u128,

    /// The amounts of token_0 and token_1 that are owed to the protocol.
    pub protocol_fees_token_0: u64,
    pub protocol_fees_token_1: u64,

    /// The amounts in and out of swap token_0 and token_1
    pub swap_in_amount_token_0: u128,
    pub swap_out_amount_token_1: u128,
    pub swap_in_amount_token_1: u128,
    pub swap_out_amount_token_0: u128,

    /// Bitwise representation of the state of the pool
    /// bit0, 1: disable open position and increase liquidity, 0: normal
    /// bit1, 1: disable decrease liquidity, 0: normal
    /// bit2, 1: disable collect fee, 0: normal
    /// bit3, 1: disable collect reward, 0: normal
    /// bit4, 1: disable swap, 0: normal
    pub status: u8,
    /// Leave blank for future use
    pub padding: [u8; 7],

    pub reward_infos: [RewardInfo; REWARD_NUM],

    /// Packed initialized tick array state
    pub tick_array_bitmap: [u64; 16],

    /// except protocol_fee and fund_fee
    pub total_fees_token_0: u64,
    /// except protocol_fee and fund_fee
    pub total_fees_claimed_token_0: u64,
    pub total_fees_token_1: u64,
    pub total_fees_claimed_token_1: u64,

    pub fund_fees_token_0: u64,
    pub fund_fees_token_1: u64,

    // The timestamp allowed for swap in the pool.
    // Note: The open_time is disabled for now.
    pub open_time: u64,
    // account recent update epoch
    pub recent_epoch: u64,

    // Unused bytes for future upgrades.
    pub padding1: [u64; 24],
    pub padding2: [u64; 32],
}

// Offsets внутри аккаунта пула CLMM (PoolState), уже с учётом первых 8 байт discriminator.
const BUMP_OFFSET: usize = 8;
const AMM_CONFIG_OFFSET: usize = 9;
const OWNER_OFFSET: usize = 41;
const MINT_A_OFFSET: usize = 73;       // token_mint_0
const MINT_B_OFFSET: usize = 105;      // token_mint_1
const VAULT_A_OFFSET: usize = 137;     // token_vault_0
const VAULT_B_OFFSET: usize = 169;     // token_vault_1
const OBSERVATION_KEY_OFFSET: usize = 201;
const DECIMALS_A_OFFSET: usize = 233;  // mint_decimals_0
const DECIMALS_B_OFFSET: usize = 234;  // mint_decimals_1
const TICK_SPACING_OFFSET: usize = 235; // u16
const LIQUIDITY_OFFSET: usize = 237;   // u128, 237..253
const SQRT_PRICE_X64_OFFSET: usize = 253; // u128, 253..269
const TICK_CURRENT_OFFSET: usize = 269;   // i32, 269..273

/// Минимальная структура CLMM-пула, достаточная для off-chain расчётов арбитража.
pub struct RaydiumClmmPoolInfo {
    pub pubkey: Pubkey,
    pub amm_config: Pubkey,
    mint_a: Pubkey,
    mint_b: Pubkey,
    pub vault_a: Pubkey,
    pub vault_b: Pubkey,
    pub decimals_a: u8,
    pub decimals_b: u8,
    pub tick_spacing: u16,
    pub liquidity: u128,
    pub sqrt_price_x64: u128,
    pub tick_current: i32,
    /// Комиссия пула (trade fee) в basis points, например 25 = 0.25%
    pub fee_rate_bps: u16,
}

impl PoolMints for RaydiumClmmPoolInfo {
    fn pool_pubkey(&self) -> &Pubkey {
        &self.pubkey
    }

    fn mint_a(&self) -> &Pubkey {
        &self.mint_a
    }

    fn mint_b(&self) -> &Pubkey {
        &self.mint_b
    }

    /// Упрощённый расчёт amount_out для небольших свопов на текущем тике.
    ///
    /// Для полноценной реализации нужен перебор tick array и распределения ликвидности,
    /// но для оценки арбитража на малых объёмах можно использовать локальную модель
    /// на основе текущего sqrt_price_x64 и liquidity.
    fn amount_out(
        &self,
        _client: &RpcClient,
        amount_in: u64,
        token_in: &Pubkey,
    ) -> Result<u64, Box<dyn std::error::Error>> {
        if amount_in == 0 || self.liquidity == 0 {
            return Err("Amount in is 0 or liquidity is 0".into());
        }

        // Применяем комиссию пула к входящему количеству.
        let fee_bps = self.fee_rate_bps as u128;
        let amount_in_u128 = amount_in as u128;
        let amount_in_after_fee = amount_in_u128 * (10_000u128 - fee_bps) / 10_000u128;

        // В упрощённой модели предполагаем своп в пределах текущего тика,
        // без перехода через границы тиков. Используем формулы Uniswap v3:
        //
        // Для свопа token0 -> token1:
        //   amount_out = L * (sqrtP - sqrtP_new)
        //   amount_in  = L * (1/sqrtP_new - 1/sqrtP)
        //
        // Для малых amount_in можно аппроксимировать локальным производным,
        // что эквивалентно использованию текущей цены без сильного сдвига sqrtP.

        let sqrt_p = self.sqrt_price_x64 as f64 / (2u128.pow(64) as f64);
        if sqrt_p == 0.0 {
            return Err("Sqrt price is 0".into());
        }

        // Текущая цена token_b / token_a.
        let price = (sqrt_p * sqrt_p)
            * 10f64.powi((self.decimals_a as i32 - self.decimals_b as i32) as i32);
        if price == 0.0 {
            return Err("Price is 0".into());
        }

        // В локальной линейной аппроксимации:
        // amount_out ≈ amount_in_after_fee * price или обратное, в зависимости от направления.
        let amount_in_f = amount_in_after_fee as f64;
       
        let amount_out_f = if *token_in == *self.mint_a() {  // a -> b
            amount_in_f * price * 10f64.powi((self.decimals_b as i32 - self.decimals_a as i32) as i32)
        } else if *token_in == *self.mint_b() { // b -> a
            amount_in_f / price * 10f64.powi((self.decimals_a as i32 - self.decimals_b as i32) as i32)
        } else {
            return Err("Token in is not mint_a or mint_b".into());
        };

        if amount_out_f <= 0.0 {
            return Err("Amount out is less than 0".into());
        } else {
            Ok(amount_out_f as u64)
        }
    }
}

impl RaydiumClmmPoolInfo {
    /// Создать структуру пула из бинарных данных аккаунта PoolState.
    pub fn create(pool_pubkey: Pubkey, client: &RpcClient) -> Result<Self, Box<dyn std::error::Error>> {
        let account = client.get_account(&pool_pubkey)?;

        let amm_config = Pubkey::new_from_array(
            account.data[AMM_CONFIG_OFFSET..AMM_CONFIG_OFFSET + 32].try_into().unwrap(),
        );

        let mint_a = Pubkey::new_from_array(
            account.data[MINT_A_OFFSET..MINT_A_OFFSET + 32].try_into().unwrap(),
        );
        let mint_b = Pubkey::new_from_array(
            account.data[MINT_B_OFFSET..MINT_B_OFFSET + 32].try_into().unwrap(),
        );
        let vault_a = Pubkey::new_from_array(
            account.data[VAULT_A_OFFSET..VAULT_A_OFFSET + 32].try_into().unwrap(),
        );
        let vault_b = Pubkey::new_from_array(
            account.data[VAULT_B_OFFSET..VAULT_B_OFFSET + 32].try_into().unwrap(),
        );

        let tick_spacing_bytes: [u8; 2] = account.data[TICK_SPACING_OFFSET..TICK_SPACING_OFFSET + 2].try_into()?;
        let tick_spacing = u16::from_le_bytes(tick_spacing_bytes);

        let liquidity_bytes: [u8; 16] = account.data[LIQUIDITY_OFFSET..LIQUIDITY_OFFSET + 16].try_into()?;
        let liquidity = u128::from_le_bytes(liquidity_bytes);

        let sqrt_price_bytes: [u8; 16] = account.data[SQRT_PRICE_X64_OFFSET..SQRT_PRICE_X64_OFFSET + 16].try_into()?;
        let sqrt_price_x64 = u128::from_le_bytes(sqrt_price_bytes);

        let tick_current_bytes: [u8; 4] = account.data[TICK_CURRENT_OFFSET..TICK_CURRENT_OFFSET + 4].try_into()?;
        let tick_current = i32::from_le_bytes(tick_current_bytes);

        // Десятичные разряды читаем из mint-аккаунтов, а не из PoolState,
        // чтобы быть совместимыми с AMM-частью и унифицировать логику.
        let mint_a_acc = client.get_account(&mint_a)?;
        let mint_b_acc = client.get_account(&mint_b)?;
        let decimals_a = read_mint_decimals(&mint_a_acc) as u8;
        let decimals_b = read_mint_decimals(&mint_b_acc) as u8;

        // Читаем fee_rate из AmmConfig аккаунта.
        let fee_rate_bps = read_clmm_fee_rate_bps(client, &amm_config)?;

        debug!(
            "Parsed CLMM Pool: \
             \n\tmintA={}, \
             \n\tmintB={}, \
             \n\tvaultA={}, \
             \n\tvaultB={}, \
             \n\tamm_config={}, \
             \n\tliquidity={}, \
             \n\tsqrtPriceX64={}, \
             \n\ttick_current={}, \
             \n\ttick_spacing={}, \
             \n\tfee_bps={}",
            mint_a,
            mint_b,
            vault_a,
            vault_b,
            amm_config,
            liquidity,
            sqrt_price_x64,
            tick_current,
            tick_spacing,
            fee_rate_bps
        );

        Ok(Self {
            pubkey: pool_pubkey,
            amm_config,
            mint_a,
            mint_b,
            vault_a,
            vault_b,
            decimals_a,
            decimals_b,
            tick_spacing,
            liquidity,
            sqrt_price_x64,
            tick_current,
            fee_rate_bps,
        })
    }

    /// Посчитать текущую цену quote/base на основе sqrt_price_x64.
    /// Получает свежие данные пула перед расчётом цены.
    pub fn price(&self, client: &RpcClient) -> Result<f64, Box<dyn std::error::Error>> {
        // Получаем свежие данные пула для актуального sqrt_price_x64
        let account = client.get_account(&self.pubkey)?;
        
        let sqrt_price_bytes: [u8; 16] =
            account.data[SQRT_PRICE_X64_OFFSET..SQRT_PRICE_X64_OFFSET + 16].try_into()?;
        let sqrt_price_x64 = u128::from_le_bytes(sqrt_price_bytes);
        
        let sqrt_price = (sqrt_price_x64 as f64) / (2u128.pow(64) as f64);
        let decimals_diff = (self.decimals_a as i32 - self.decimals_b as i32) as i32;
        let price = (sqrt_price * sqrt_price) * 10f64.powi(decimals_diff);

        debug!(
            "\nPool Ray CLMM {} -> \
             \n\tsqrtPriceX64: {} \
             \n\t\tprice: {}",
            self.pubkey, sqrt_price_x64, price
        );

        Ok(price)
    }
}

/// Чтение trade_fee_rate (fee в bps) из AmmConfig аккаунта.
/// 
/// Структура AmmConfig (после 8-байтового discriminator):
/// - bump: u8 (offset 8)
/// - index: u16 (offset 9)
/// - owner: Pubkey (offset 11, 32 байта)
/// - protocol_fee_rate: u32 (offset 43)
/// - trade_fee_rate: u32 (offset 47) <- это поле
/// 
/// trade_fee_rate хранится как u32 в формате "hundredths of a bip" (10^-6),
/// конвертируем в basis points: value / 100.
fn read_clmm_fee_rate_bps(
    client: &RpcClient,
    amm_config: &Pubkey,
) -> Result<u16, Box<dyn std::error::Error>> {
    let acc = client.get_account(amm_config)?;
    let data = acc.data;

    // Правильный offset для trade_fee_rate (u32) в структуре AmmConfig
    // discriminator (8) + bump (1) + index (2) + owner (32) + protocol_fee_rate (4) = 47
    const TRADE_FEE_RATE_OFFSET: usize = 47;
    if data.len() >= TRADE_FEE_RATE_OFFSET + 4 {
        let raw: [u8; 4] = data[TRADE_FEE_RATE_OFFSET..TRADE_FEE_RATE_OFFSET + 4].try_into()?;
        let trade_fee_rate_u32 = u32::from_le_bytes(raw);
        // Конвертация из hundredths of a bip (10^-6) в basis points (10^-4)
        // Например: 2500 hundredths of a bip = 0.25% = 25 bps
        let fee_bps = (trade_fee_rate_u32 / 100) as u16;
        Ok(fee_bps)
    } else {
        // Fallback: 25 bps как типичная торговая комиссия Raydium.
        Ok(25)
    }
}

/// Основная функция для получения информации о CLMM-пуле.
pub fn get_info_clmm(
    client: &RpcClient,
    pool_key: &str,
) -> Result<RaydiumClmmPoolInfo, Box<dyn std::error::Error>> {
    let pool_pubkey: Pubkey = pool_key.parse()?;
    debug!("Fetching Raydium CLMM pool {}", pool_pubkey);

    let pool_info = RaydiumClmmPoolInfo::create(pool_pubkey, client)?;
    Ok(pool_info)
}
