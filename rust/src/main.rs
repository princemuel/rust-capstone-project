#![allow(unused)]
use std::fs::File;
use std::io::Write;

use bitcoin::hex::DisplayHex;
use bitcoincore_rpc::bitcoin::{Address, Amount, Network};
use bitcoincore_rpc::json::AddressType;
use bitcoincore_rpc::jsonrpc::error::RpcError;
use bitcoincore_rpc::{Auth, Client, RpcApi, bitcoin, jsonrpc};
use serde::Deserialize;
use serde_json::json;

// Node access params
const RPC_URL: &str = "http://127.0.0.1:18443"; // Default regtest RPC port
const RPC_USER: &str = "alice";
const RPC_PASS: &str = "password";

// You can use calls not provided in RPC lib API using the generic `call`
// function. An example of using the `send` RPC call, which doesn't have exposed
// API. You can also use serde_json `Deserialize` derivation to capture the
// returned json result.
fn send(rpc: &Client, addr: &str) -> bitcoincore_rpc::Result<String> {
    let args = [
        json!([{addr : 100 }]), // recipient address
        json!(null),            // conf target
        json!(null),            // estimate mode
        json!(null),            // fee rate in sats/vb
        json!(null),            // Empty option object
    ];

    #[derive(Deserialize)]
    struct SendResult {
        complete: bool,
        txid: String,
    }
    let send_result = rpc.call::<SendResult>("send", &args)?;
    assert!(send_result.complete);
    Ok(send_result.txid)
}

fn main() -> bitcoincore_rpc::Result<()> {
    // Connect to Bitcoin Core RPC
    let rpc = Client::new(
        RPC_URL,
        Auth::UserPass(RPC_USER.to_owned(), RPC_PASS.to_owned()),
    )?;

    // Get blockchain info
    let blockchain_info = rpc.get_blockchain_info()?;
    println!("Blockchain Info: {blockchain_info:?}");

    // Load or Create the relevant Wallets
    if rpc.load_wallet("Miner").is_err() {
        rpc.create_wallet("Miner", None, None, None, None)?;
    }
    if rpc.load_wallet("Trader").is_err() {
        rpc.create_wallet("Trader", None, None, None, None)?;
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

    let mut blocks_mined = 0;
    let mut balance = Amount::ZERO;

    println!("Mining blocks until we get a positive spendable balance...");
    while balance == Amount::ZERO {
        // Mine one block at a time
        miner_rpc.generate_to_address(1, &miner_address)?;
        blocks_mined += 1;

        // Check spendable balance (excluding immature coinbase)
        balance = miner_rpc.get_balance(None, None)?;
        if blocks_mined % 10 == 0 {
            println!(
                "Mined {} blocks, spendable balance: {} BTC",
                blocks_mined,
                balance.to_btc()
            );
        }
    }

    // In Bitcoin, newly mined coinbase transactions have a maturity period of 100
    // blocks. This means the coinbase reward from a block can only be spent
    // after 100 more blocks have been mined on top of it. The first block's
    // reward becomes spendable after block 101 is mined, which is why it takes
    // exactly 101 blocks to get a positive spendable balance in a fresh regtest
    // environment.
    println!(
        "Mined {} blocks to get positive spendable balance of {} BTC",
        blocks_mined,
        balance.to_btc()
    );
    println!("Miner wallet balance: {} BTC", balance.to_btc());

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
    let tx_info = miner_rpc.get_transaction(&txid, None)?;
    let raw_tx = rpc.get_raw_transaction(&txid, None)?;

    // Get block information
    let best_block_hash = rpc.get_best_block_hash()?;
    let block_height = rpc.get_block_count()?;
    let block_info = rpc.get_block(&best_block_hash)?;

    // Extract transaction details
    let mut input_address = String::with_capacity(42);
    let mut input_amount = 0.0;

    let mut output_address = String::with_capacity(42);
    let mut output_amount = 0.0;

    let mut change_address = String::with_capacity(42);
    let mut change_amount = 0.0;

    // Get input details: via the previous transaction outputs
    if let Some(input) = raw_tx.input.first() {
        let prev_tx = rpc.get_raw_transaction(&input.previous_output.txid, None)?;
        let prev_output = &prev_tx.output[input.previous_output.vout as usize];
        input_amount = prev_output.value.to_btc();

        // Get the address from the script
        if let Ok(address) = Address::from_script(&prev_output.script_pubkey, Network::Regtest)
        {
            input_address = address.to_string();
        }
    }

    // Get output details
    for output in &raw_tx.output {
        let address = match Address::from_script(&output.script_pubkey, Network::Regtest) {
            Ok(addr) => addr,
            Err(_) => continue,
        };

        let address = address.to_string();
        let amount = output.value.to_btc();

        if address == trader_address.to_string() {
            (output_address, output_amount) = (address, amount);
        } else {
            (change_address, change_amount) = (address, amount);
        }
    }

    // Calculate transaction fees
    let total_output: f64 = raw_tx.output.iter().map(|out| out.value.to_btc()).sum();
    let transaction_fees = input_amount - total_output;

    // Write the data to ../out.txt in the specified format given in readme.md
    let mut file = File::create("../out.txt")?;

    // Transaction ID (txid)
    writeln!(file, "{txid}")?;

    // Miner's Input Address and Amount (BTC)
    writeln!(file, "{input_address}")?;
    writeln!(file, "{input_amount}")?;

    // Trader's Input Address and Amount (BTC)
    writeln!(file, "{output_address}")?;
    writeln!(file, "{output_amount}")?;

    // Miner's Change Address and Amount (BTC)
    writeln!(file, "{change_address}")?;
    writeln!(file, "{change_amount}")?;

    // Transaction Fees (BTC)
    writeln!(file, "{transaction_fees:.2e}")?;

    // Block height at which the transaction is confirmed
    writeln!(file, "{block_height}")?;

    // Block hash at which the transaction is confirmed
    writeln!(file, "{best_block_hash}")?;

    println!("Transaction details written to out.txt");

    Ok(())
}
