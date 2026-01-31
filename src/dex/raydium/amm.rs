use solana_client::rpc_client::RpcClient;
use solana_sdk::pubkey::Pubkey;
use log::debug;

use crate::common::{read_spl_amount, read_u64};
use crate::dex::PoolMints;

const BASE_VAULT_OFFSET: usize = 336; // coinVault/tokenVaultA
const QUOTE_VAULT_OFFSET: usize = 368; // pcVault/tokenVaultB
const BASE_MINT_OFFSET: usize = 400; // coinMint/tokenMintA
const QUOTE_MINT_OFFSET: usize = 432; // pcMint/tokenMintB
const FEES_OFFSET: usize = 128; // fees struct starts after sys_decimal_value (offset 128 + 8 bytes)

/*
В структуре Raydium AMM pnl_fee (комиссия протокола) берется не от всей суммы транзакции, а от уже начисленной торговой комиссии (trade_fee).
В Raydium v4 значения trade_fee_numerator и swap_fee_numerator обычно идентичны (как в вашем случае — 25).
trade_fee — общая комиссия.
swap_fee — используется в коде для проверки минимального остатка, но фактически для пользователя итоговая потеря составляет именно 0.25%. */

#[allow(unused)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Fees {
    /// numerator of the min_separate
    pub min_separate_numerator: u64,
    /// denominator of the min_separate
    pub min_separate_denominator: u64,

    /// numerator of the fee
    pub trade_fee_numerator: u64,
    /// denominator of the fee
    /// and 'trade_fee_denominator' must be equal to 'min_separate_denominator'
    pub trade_fee_denominator: u64,

    /// numerator of the pnl
    pub pnl_numerator: u64,
    /// denominator of the pnl
    pub pnl_denominator: u64,

    /// numerator of the swap_fee
    pub swap_fee_numerator: u64,
    /// denominator of the swap_fee
    pub swap_fee_denominator: u64,
}

/// Чтение структуры Fees из бинарных данных аккаунта
fn read_fees(data: &[u8]) -> Fees {
    Fees {
        min_separate_numerator: read_u64(data, FEES_OFFSET),
        min_separate_denominator: read_u64(data, FEES_OFFSET + 8),
        trade_fee_numerator: read_u64(data, FEES_OFFSET + 16),
        trade_fee_denominator: read_u64(data, FEES_OFFSET + 24),
        pnl_numerator: read_u64(data, FEES_OFFSET + 32),
        pnl_denominator: read_u64(data, FEES_OFFSET + 40),
        swap_fee_numerator: read_u64(data, FEES_OFFSET + 48),
        swap_fee_denominator: read_u64(data, FEES_OFFSET + 56),
    }
}

#[allow(unused)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct StateData {
    /// delay to take pnl coin
    pub need_take_pnl_coin: u64,
    /// delay to take pnl pc
    pub need_take_pnl_pc: u64,
    /// total pnl pc
    pub total_pnl_pc: u64,
    /// total pnl coin
    pub total_pnl_coin: u64,
    /// ido pool open time
    pub pool_open_time: u64,
    /// padding for future updates
    pub padding: [u64; 2],
    /// switch from orderbookonly to init
    pub orderbook_to_init_time: u64,

    /// swap coin in amount
    pub swap_coin_in_amount: u128,
    /// swap pc out amount
    pub swap_pc_out_amount: u128,
    /// charge pc as swap fee while swap pc to coin
    pub swap_acc_pc_fee: u64,

    /// swap pc in amount
    pub swap_pc_in_amount: u128,
    /// swap coin out amount
    pub swap_coin_out_amount: u128,
    /// charge coin as swap fee while swap coin to pc
    pub swap_acc_coin_fee: u64,
}

#[allow(unused)]
#[derive(Clone, Copy, Default, PartialEq)]
pub struct AmmInfo {
    /// Initialized status.
    pub status: u64,
    /// Nonce used in program address.
    /// The program address is created deterministically with the nonce,
    /// amm program id, and amm account pubkey.  This program address has
    /// authority over the amm's token coin account, token pc account, and pool
    /// token mint.
    pub nonce: u64,
    /// max order count
    pub order_num: u64,
    /// within this range, 5 => 5% range
    pub depth: u64,
    /// coin decimal
    pub coin_decimals: u64,
    /// pc decimal
    pub pc_decimals: u64,
    /// amm machine state
    pub state: u64,
    /// amm reset_flag
    pub reset_flag: u64,
    /// min size 1->0.000001
    pub min_size: u64,
    /// vol_max_cut_ratio numerator, sys_decimal_value as denominator
    pub vol_max_cut_ratio: u64,
    /// amount wave numerator, sys_decimal_value as denominator
    pub amount_wave: u64,
    /// coinLotSize 1 -> 0.000001
    pub coin_lot_size: u64,
    /// pcLotSize 1 -> 0.000001
    pub pc_lot_size: u64,
    /// min_cur_price: (2 * amm.order_num * amm.pc_lot_size) * max_price_multiplier
    pub min_price_multiplier: u64,
    /// max_cur_price: (2 * amm.order_num * amm.pc_lot_size) * max_price_multiplier
    pub max_price_multiplier: u64,
    /// system decimal value, used to normalize the value of coin and pc amount
    pub sys_decimal_value: u64,
    /// All fee information
    pub fees: Fees,
    /// Statistical data
    pub state_data: StateData,
    /// Coin vault
    pub coin_vault: Pubkey,
    /// Pc vault
    pub pc_vault: Pubkey,
    /// Coin vault mint
    pub coin_vault_mint: Pubkey,
    /// Pc vault mint
    pub pc_vault_mint: Pubkey,
    /// lp mint
    pub lp_mint: Pubkey,
    /// open_orders key
    pub open_orders: Pubkey,
    /// market key
    pub market: Pubkey,
    /// market program key
    pub market_program: Pubkey,
    /// target_orders key
    pub target_orders: Pubkey,
    /// padding
    pub padding1: [u64; 8],
    /// amm owner key
    pub amm_owner: Pubkey,
    /// pool lp amount
    pub lp_amount: u64,
    /// client order id
    pub client_order_id: u64,
    /// recent epoch
    pub recent_epoch: u64,
    /// padding
    pub padding2: u64,
}

pub trait CheckedCeilDiv: Sized {
    /// Perform ceiling division
    fn checked_ceil_div(&self, rhs: Self) -> Option<Self>;
}

impl CheckedCeilDiv for u128 {
    fn checked_ceil_div(&self, rhs: Self) -> Option<Self> {
        let mut quotient = self.checked_div(rhs)?;
        let remainder = self.checked_rem(rhs)?;
        if remainder != 0 {
            quotient = quotient.checked_add(1)?;
        }
        Some(quotient)
    }
}

pub struct RaydiumAmmPoolInfo {
    pub pubkey : Pubkey,
    pub base_vault: Pubkey,
    pub quote_vault: Pubkey,
    base_mint: Pubkey,
    quote_mint: Pubkey,
    pub fees: Fees,
}

impl PoolMints for RaydiumAmmPoolInfo {
    fn pool_pubkey(&self) -> &Pubkey {
        &self.pubkey
    }

    fn mint_a(&self) -> &Pubkey {
        &self.base_mint
    }

    fn mint_b(&self) -> &Pubkey {
        &self.quote_mint
    }

    /// Расчёт amount_out для свопа в AMM v4 (формула x*y=k) с учётом комиссии.
    ///
    /// `amount_in` задаётся в натуральных единицах токена (u64 в минимальных долях).
    /// `token_in` определяет направление: если это `mint_a()`, то меняем mint_a -> mint_b,
    /// если `mint_b()` — наоборот.
    fn amount_out(
        &self,
        client: &RpcClient,
        amount_in: u64,
        token_in: &Pubkey,
    ) -> Result<u64, Box<dyn std::error::Error>> {
        if amount_in == 0 {
            return Ok(0);
        }

        let base_vault_acc = client.get_account(&self.base_vault)?;
        let quote_vault_acc = client.get_account(&self.quote_vault)?;

        let base_raw = read_spl_amount(&base_vault_acc) as u128;
        let quote_raw = read_spl_amount(&quote_vault_acc) as u128;

        let amount_in_u128 = amount_in as u128;

        let swap_fee = amount_in_u128
            .checked_mul(self.fees.swap_fee_numerator.into())
            .unwrap()
            .checked_ceil_div(self.fees.swap_fee_denominator.into())
            .unwrap();

        // Комиссия снимается из amount_in
        let amount_in_after_fee = amount_in_u128.saturating_sub(swap_fee);

        let (reserve_in, reserve_out) = if *token_in == *self.mint_a() {
            (base_raw, quote_raw)
        } else if *token_in == *self.mint_b() {
            (quote_raw, base_raw)
        } else {
            return Err("token_in is neither mint_a nor mint_b".into());
        };

        if reserve_in == 0 || reserve_out == 0 {
            return Ok(0);
        }

        let amount_out = (reserve_out - (reserve_in * reserve_out) / (reserve_in + amount_in_after_fee)) as u64;

        Ok(amount_out)
    }
}

// Комментарий
impl RaydiumAmmPoolInfo {
    /// Создать из бинарных данных аккаунта
    pub fn create(pool_pubkey: Pubkey, client: &RpcClient) -> Result<Self, Box<dyn std::error::Error>> {
        let account = client.get_account(&pool_pubkey)?;

        let base_vault = Pubkey::new_from_array(
            account.data[BASE_VAULT_OFFSET..BASE_VAULT_OFFSET + 32].try_into().unwrap()); // offset vaultA
        let quote_vault = Pubkey::new_from_array(
            account.data[QUOTE_VAULT_OFFSET..QUOTE_VAULT_OFFSET + 32].try_into().unwrap()); // offset vaultB
        let base_mint = Pubkey::new_from_array(
            account.data[BASE_MINT_OFFSET..BASE_MINT_OFFSET + 32].try_into().unwrap());   // offset mintA
        let quote_mint = Pubkey::new_from_array(
            account.data[QUOTE_MINT_OFFSET..QUOTE_MINT_OFFSET + 32].try_into().unwrap());  // offset mintB

        // Читаем структуру Fees из данных аккаунта
        let fees = read_fees(&account.data);

        debug!(
            "Parsed AMM Pool: \n\tmintA={}, \n\tmintB={}, \n\tvaultA={}, \n\tvaultB={}, \n\tfees={:?}",
            base_mint, quote_mint, base_vault, quote_vault, fees
        );

        Ok(Self {
            pubkey : pool_pubkey,
            base_vault,
            quote_vault,
            base_mint,
            quote_mint,
            fees,
        })
    }

    pub fn calc_swap_fee(&self, amount_in: u64) -> Result<u64, Box<dyn std::error::Error>> {
        let swap_fee = (amount_in as u128)
            .checked_mul(self.fees.swap_fee_numerator.into())
            .unwrap()
            .checked_ceil_div(self.fees.swap_fee_denominator.into())
            .unwrap() as u64;

        Ok(swap_fee)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use solana_client::rpc_client::RpcClient;
    use solana_sdk::pubkey::Pubkey;
    use std::str::FromStr;

    #[test]
    fn test_amm_pool_fees_and_amount_out() {
        // Инициализация логгера для теста
        let _ = env_logger::try_init();

        // Захардкоженный адрес Raydium AMM пула (WSOL/USDC пул)
        let pool_address_str = "Bzc9NZfMqkXR6fz1DBph7BDf9BroyEf6pnzESP7v5iiw";
        let pool_pubkey = Pubkey::from_str(pool_address_str).expect("Invalid pool address");

        // Подключаемся к RPC
        let rpc_url = "https://api.mainnet-beta.solana.com";
        let client = RpcClient::new(rpc_url.to_string());

        // Создаем RaydiumAmmPoolInfo
        let pool_info = RaydiumAmmPoolInfo::create(pool_pubkey, &client)
            .expect("Failed to create pool info");

        // Выводим полученную структуру Fees
        println!("Fees structure:");
        println!("  min_separate_numerator: {}", pool_info.fees.min_separate_numerator);
        println!("  min_separate_denominator: {}", pool_info.fees.min_separate_denominator);
        println!("  trade_fee_numerator: {}", pool_info.fees.trade_fee_numerator);
        println!("  trade_fee_denominator: {}", pool_info.fees.trade_fee_denominator);
        println!("  pnl_numerator: {}", pool_info.fees.pnl_numerator);
        println!("  pnl_denominator: {}", pool_info.fees.pnl_denominator);
        println!("  swap_fee_numerator: {}", pool_info.fees.swap_fee_numerator);
        println!("  swap_fee_denominator: {}", pool_info.fees.swap_fee_denominator);

        // Захардкоженное значение amount_in (например, 1 SOL = 1_000_000_000 lamports)
        let amount_in: u64 = 1_000_000_000;

        // Вычисляем amount_out для свопа base_mint -> quote_mint
        // Выходное количество без учета запятой. Она не нужна при расчете арбитража
        let amount_out = pool_info.amount_out(&client, amount_in, pool_info.mint_a())
            .expect("Failed to calculate amount_out");

        let swap_fee = pool_info.calc_swap_fee(amount_in).expect("Can't calc swap fee");

        println!("\nSwap calculation:");
        println!("  amount_in: {} (base token)", amount_in);
        println!("  amount_out: {} (quote token)", amount_out);
        println!("  swap_fee: {}", swap_fee);
    }
}