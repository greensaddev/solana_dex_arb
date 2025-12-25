use solana_sdk::pubkey::Pubkey;
use solana_sdk::account::Account;
use solana_client::rpc_client::RpcClient;
use std::str::FromStr;

// Чтение u64 (LE)
pub fn read_u64(data: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes(data[offset..offset + 8].try_into().unwrap())
}

// Чтение amount из SPL Token Account
pub fn read_spl_amount(acc: &Account) -> u64 {
    read_u64(&acc.data, 64)
}

// Чтение decimals из Mint Account
pub fn read_mint_decimals(acc: &Account) -> u8 {
    acc.data[44]
}