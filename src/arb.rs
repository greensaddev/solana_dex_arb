use solana_sdk::pubkey::Pubkey;
use solana_client::rpc_client::RpcClient;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use log::info;
use crate::dex::PoolMints;

/// Строит граф арбитража на основе HashMap пулов.
/// 
/// # Arguments
/// * `start_mint` - начальный mint токена
/// * `start_amount` - количество токенов начального минта
/// * `pools_map` - HashMap, где ключ - mint адрес, значение - вектор пулов, содержащих этот mint
/// * `client` - RPC клиент для получения актуальных данных пулов
/// 
/// # Returns
/// Вектор цепочек арбитража. Каждая цепочка - это последовательность пулов (Vec<Arc<dyn PoolMints>>),
/// представляющая путь от начального минта обратно к начальному минту через серию свопов.
/// 
/// # Правила построения графа:
/// 1. Максимум 4 обмена (свапа) в цепочке
/// 2. Первый пул в цепочке должен быть связан с начальным минтом
/// 3. Пулы в цепочке не должны повторяться (Pubkey этих пулов должны быть уникальными)
/// 4. Завершаться цепочка должна получением токена, минт которого совпадает с начальным
pub fn build_arbitrage_graph(
    start_mint: &Pubkey,
    start_amount: u64,
    pools_map: &HashMap<Pubkey, Vec<Arc<dyn PoolMints>>>,
    client: &RpcClient,
) -> Result<Vec<Vec<Arc<dyn PoolMints>>>, Box<dyn std::error::Error>> {
    info!("Starting arbitrage graph building");
    info!("Start mint: {}, Start amount: {}", start_mint, start_amount);
    info!("Available mints in pools_map: {}", pools_map.len());
    
    let mut result: Vec<Vec<Arc<dyn PoolMints>>> = Vec::new();
    let mut current_path: Vec<Arc<dyn PoolMints>> = Vec::new();
    let mut used_pools: HashSet<Pubkey> = HashSet::new();

    // Вспомогательная рекурсивная функция для поиска цепочек
    fn dfs(
        current_mint: &Pubkey,
        current_amount: u64,
        start_mint: &Pubkey,
        start_amount: u64,
        pools_map: &HashMap<Pubkey, Vec<Arc<dyn PoolMints>>>,
        client: &RpcClient,
        current_path: &mut Vec<Arc<dyn PoolMints>>,
        used_pools: &mut HashSet<Pubkey>,
        depth: usize,
        max_depth: usize,
        result: &mut Vec<Vec<Arc<dyn PoolMints>>>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // Если достигли максимальной глубины, прекращаем поиск
        if depth >= max_depth {
            return Ok(());
        }

        // Получаем все пулы, которые содержат текущий mint
        let pools = match pools_map.get(current_mint) {
            Some(pools) => pools,
            None => return Ok(()), // Нет пулов для этого минта
        };

        // Перебираем все доступные пулы
        for pool in pools {
            let pool_pubkey = *pool.pool_pubkey();

            // Проверяем, что пул еще не использован в текущей цепочке
            if used_pools.contains(&pool_pubkey) {
                continue;
            }

            // Определяем, какой токен мы можем обменять в этом пуле
            let (token_in, token_out) = if *pool.mint_a() == *current_mint {
                (*pool.mint_a(), *pool.mint_b())
            } else if *pool.mint_b() == *current_mint {
                (*pool.mint_b(), *pool.mint_a())
            } else {
                continue; // Этот пул не содержит текущий mint (не должно происходить, но на всякий случай)
            };

            // Рассчитываем количество выходных токенов
            let amount_out = match pool.amount_out(client, current_amount, &token_in) {
                Ok(amount) => amount,
                Err(_) => continue, // Пропускаем пул, если не удалось рассчитать amount_out
            };

            if amount_out == 0 {
                continue; // Пропускаем пулы с нулевым выходом
            }

            // Добавляем пул в текущий путь
            current_path.push(Arc::clone(pool));
            used_pools.insert(pool_pubkey);

            // Проверяем, вернулись ли мы к начальному минту
            if token_out == *start_mint {
                if (amount_out > start_amount) {
                    // Нашли завершенную цепочку арбитража
                    result.push(current_path.clone());
                    info!("Found arbitrage chain #{} with {} pools:", result.len(), current_path.len());
                    
                    // Пересчитываем путь для детального логирования
                    let mut chain_amount = start_amount;
                    let mut current_token = *start_mint;
                    
                    for (idx, pool) in current_path.iter().enumerate() {
                        let pool_pubkey = pool.pool_pubkey();
                        let mint_a = pool.mint_a();
                        let mint_b = pool.mint_b();
                        
                        // Определяем направление обмена
                        let (token_in, token_out) = if *mint_a == current_token {
                            (*mint_a, *mint_b)
                        } else if *mint_b == current_token {
                            (*mint_b, *mint_a)
                        } else {
                            // Это не должно происходить, но на всякий случай
                            info!("  Step {}: Pool {} - ERROR: token mismatch", idx + 1, pool_pubkey);
                            break;
                        };
                        
                        // Рассчитываем amount_out для логирования
                        let amount_out = match pool.amount_out(client, chain_amount, &token_in) {
                            Ok(amt) => amt,
                            Err(e) => {
                                info!("  Step {}: Pool {} - ERROR calculating amount_out: {}", idx + 1, pool_pubkey, e);
                                break;
                            }
                        };
                        
                        info!("  Step {}", 
                            idx + 1
                        );

                        info!("    Pool {} ", 
                            pool_pubkey
                        );

                        info!("    {} -> {}", 
                            token_in, 
                            token_out,
                        );

                        info!("    amount_in: {}, amount_out: {}", 
                            chain_amount,
                            amount_out
                        );
                        
                        chain_amount = amount_out;
                        current_token = token_out;
                    }
                    
                    let profit: i64 = chain_amount as i64 - start_amount as i64;
                    info!("  Chain summary: start_amount={}, final_amount={}, profit={}", 
                        start_amount, chain_amount, profit);
                }  
            } else {
                // Продолжаем поиск с новым токеном
                dfs(
                    &token_out,
                    amount_out,
                    start_mint,
                    start_amount,
                    pools_map,
                    client,
                    current_path,
                    used_pools,
                    depth + 1,
                    max_depth,
                    result,
                )?;
            }

            // Откатываем изменения (backtracking)
            current_path.pop();
            used_pools.remove(&pool_pubkey);
        }

        Ok(())
    }

    // Запускаем поиск с начального минта
    dfs(
        start_mint,
        start_amount,
        start_mint,
        start_amount,
        pools_map,
        client,
        &mut current_path,
        &mut used_pools,
        0,
        4, // Максимум 4 обмена
        &mut result,
    )?;

    info!("Arbitrage graph building completed. Found {} chains", result.len());
    if result.is_empty() {
        info!("No arbitrage opportunities found for mint {} with amount {}", start_mint, start_amount);
    } else {
        info!("=== Summary of all arbitrage chains ===");
        for (chain_idx, chain) in result.iter().enumerate() {
            info!("Chain #{}: {} pools", chain_idx + 1, chain.len());
            for (pool_idx, pool) in chain.iter().enumerate() {
                info!("  Pool {}: {}", pool_idx + 1, pool.pool_pubkey());
            }
        }
    }

    Ok(result)
}