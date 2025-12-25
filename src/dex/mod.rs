pub mod raydium;

use solana_client::rpc_client::RpcClient;
use solana_sdk::pubkey::Pubkey;

/// Общий trait для всех структур пулов, предоставляющий доступ к mint-адресам токенов
/// и расчету выходного количества токенов при свопе
pub trait PoolMints {
    /// Возвращает адрес пула (pubkey)
    fn pool_pubkey(&self) -> &Pubkey;
    
    /// Возвращает адрес первого токена в паре (mint_a)
    fn mint_a(&self) -> &Pubkey;
    
    /// Возвращает адрес второго токена в паре (mint_b)
    fn mint_b(&self) -> &Pubkey;
    
    /// Рассчитывает количество выходных токенов при свопе
    /// 
    /// # Arguments
    /// * `client` - RPC клиент для получения актуальных данных пула
    /// * `amount_in` - количество входящих токенов (в минимальных единицах)
    /// * `token_in` - адрес mint токена, который входит в своп
    /// 
    /// # Returns
    /// Количество выходных токенов (в минимальных единицах) или ошибка
    fn amount_out(
        &self,
        client: &RpcClient,
        amount_in: u64,
        token_in: &Pubkey,
    ) -> Result<u64, Box<dyn std::error::Error>>;
}