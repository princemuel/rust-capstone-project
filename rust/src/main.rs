use std::fs::File;
use std::io::Write;

use bitcoincore_rpc::bitcoin::{Address, Amount, Network};
use bitcoincore_rpc::{Auth, Client, RpcApi};

// Node access params
const RPC_URL: &str = "http://127.0.0.1:18443"; // Default regtest RPC port
const RPC_USER: &str = "alice";
const RPC_PASS: &str = "password";

fn main() -> bitcoincore_rpc::Result<()> {
    // Connect to Bitcoin Core RPC
    let rpc = Client::new(
        RPC_URL,
        Auth::UserPass(RPC_USER.to_owned(), RPC_PASS.to_owned()),
    )?;

    // Get blockchain info
    let blockchain_info = rpc.get_blockchain_info()?;
    println!("Blockchain Info: {blockchain_info:#?}");

    let wallets = rpc.list_wallets()?;
    for wallet in ["Miner", "Trader"] {
        if !wallets.contains(&wallet.to_string()) {
            rpc.create_wallet(wallet, None, None, None, None)?;
        } else {
            rpc.load_wallet(wallet)?;
        }
    }

    // Create a separate RPC client for each wallet
    let miner_rpc = Client::new(
        &format!("{RPC_URL}/wallet/Miner"),
        Auth::UserPass(RPC_USER.to_owned(), RPC_PASS.to_owned()),
    )?;

    let trader_rpc = Client::new(
        &format!("{RPC_URL}/wallet/Trader"),
        Auth::UserPass(RPC_USER.to_owned(), RPC_PASS.to_owned()),
    )?;

    // Load Miner Wallet and generate a new address
    let miner_address = miner_rpc.get_new_address(Some("Mining Reward"), None)?;
    let miner_address = miner_address.assume_checked();

    // When you mine a block, you get a "coinbase reward" (free Bitcoin for mining).
    // BUT - there's a catch! You can't spend this reward immediately.
    // Bitcoin has a safety rule: you must wait for 100 MORE blocks to be mined
    // before you can spend your reward. This prevents cheating.
    //
    // Example timeline:
    // - Block 1: You mine it, get reward, but can't spend it yet
    // - Blocks 2-100: Other blocks get mined (99 blocks)
    // - Block 101: NOW you can finally spend your reward from block 1!
    //
    // This is why we need to mine 101 blocks total to see spendable money.
    println!("Mining blocks until we get a positive spendable balance...");

    let (blocks_mined, balance) = {
        // Use an infinite range (0, 1, 2, 3...) to count mining attempts
        // find_map() will keep trying each number until we get a positive balance
        (0..).find_map(|count| {
            // Step 1: Mine exactly one block and send reward to our address
            miner_rpc.generate_to_address(1, &miner_address).ok()?;

            // Step 2: Check how much money we can actually spend right now
            // (This excludes "locked" coinbase rewards that aren't mature yet)
            let balance = miner_rpc.get_balance(None, None).ok()?;

            // Step 3: If we finally have spendable money, return it!
            // Otherwise, keep mining more blocks
            #[allow(clippy::unnecessary_lazy_evaluations)]
            (balance > Amount::ZERO).then(|| (count, balance))
        })
    }
    .unwrap_or((0, Amount::ZERO)); // Fallback if something goes wrong

    let (total_blocks, balance_btc) = (blocks_mined, balance.to_btc());
    println!("Success! Mined {total_blocks} blocks to get {balance_btc} BTC");
    println!("Our wallet now has: {balance_btc} BTC available");

    // Load Trader Wallet and generate a new address
    let trader_address = trader_rpc.get_new_address(Some("Received"), None)?;
    let trader_address = trader_address.assume_checked();

    let send_amount = Amount::from_int_btc(20);
    let txid = miner_rpc.send_to_address(
        &trader_address,
        send_amount,
        None,
        None,
        None,
        None,
        None,
        None,
    )?;
    println!("Sent transaction with txid: {}", &txid);

    // Check transaction in mempool
    let mempool_entry = rpc.get_mempool_entry(&txid)?;
    println!("Transaction in mempool: {mempool_entry:#?}");

    // Mine 1 block to confirm the transaction
    let _confirmation_block = miner_rpc.generate_to_address(1, &miner_address)?;
    println!("Mined 1 confirmation block");

    // Extract all required transaction details
    let raw_tx = rpc.get_raw_transaction(&txid, None)?;

    // Get block information
    let best_block_hash = rpc.get_best_block_hash()?;
    let block_height = rpc.get_block_count()?;

    // Extract transaction details
    let (mut input_address, mut input_amount) = (String::with_capacity(42), 0.0);
    let (mut output_address, mut output_amount) = (String::with_capacity(42), 0.0);
    let (mut change_address, mut change_amount) = (String::with_capacity(42), 0.0);

    // Get input details: via the previous transaction outputs
    if let Some(input) = raw_tx.input.first() {
        let prev_tx = rpc.get_raw_transaction(&input.previous_output.txid, None)?;
        let prev_output = &prev_tx.output[input.previous_output.vout as usize];
        input_amount = prev_output.value.to_btc();

        // Get the address from the script
        input_address = Address::from_script(&prev_output.script_pubkey, Network::Regtest)
            .map(|addr| addr.to_string())
            .unwrap_or_default();
    }

    // Get output details
    let trader_addr_str = trader_address.to_string();
    for output in &raw_tx.output {
        let Ok(address) = Address::from_script(&output.script_pubkey, Network::Regtest) else {
            continue;
        };

        let (addr_str, amount) = (address.to_string(), output.value.to_btc());

        if addr_str == trader_addr_str {
            (output_address, output_amount) = (addr_str, amount);
        } else {
            (change_address, change_amount) = (addr_str, amount);
        }
    }

    // Calculate transaction fees
    let total_output: f64 = raw_tx.output.iter().map(|out| out.value.to_btc()).sum();
    let transaction_fees = input_amount - total_output;

    let mut file = File::create("../out.txt")?;

    write!(
        file,
        "{txid}\n{input_address}\n{input_amount}\n{output_address}\n{output_amount}\n{change_address}\n{change_amount}\n{transaction_fees:.2e}\n{block_height}\n{best_block_hash}\n"
    )?;

    println!("Transaction details written to out.txt");

    Ok(())
}
