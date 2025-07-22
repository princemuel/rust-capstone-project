//! Bitcoin Regtest Transaction Simulator
//!
//! This program demonstrates complete Bitcoin transaction lifecycle:
//! - Wallet creation and management via RPC
//! - Block mining and coinbase maturity rules
//! - UTXO-based transaction creation and analysis
//! - Transaction confirmation and mempool mechanics
//! - Detailed transaction forensics and reporting
//!
//! Prerequisites: Bitcoin Core running with:
//! - regtest=1 (private test network)
//! - rpcuser=alice, rpcpassword=password
//! - RPC port 18443 enabled
//! - server=1 (enable RPC server)
//!
//! Learning Goals:
//! - Understanding Bitcoin's UTXO model vs account-based systems
//! - Grasping coinbase maturity and why it exists
//! - Transaction anatomy: inputs, outputs, fees
//! - How Bitcoin prevents double-spending through consensus

use std::fs::File;
use std::io::Write;

use bitcoincore_rpc::bitcoin::{Address, Amount, Network};
use bitcoincore_rpc::{Auth, Client, RpcApi};

// * NOTE: This code is heavily commented for learning purposes
// * It is a result of my research on this exercise
// * and the resulting side quests so I decided to document it
// * Each section explains both the Rust syntax AND the Bitcoin concepts
// * Future self: read the comments first, then trace through the code

// ═══════════════════════════════════════════════════════════════
// CONFIGURATION: Bitcoin Core Connection Parameters
// ═══════════════════════════════════════════════════════════════
// These constants define how we connect to our local Bitcoin node
// Think of this like database connection strings - we need endpoint + credentials

const RPC_URL: &str = "http://127.0.0.1:18443"; // Regtest default port (mainnet=8332, testnet=18332)
const RPC_USER: &str = "alice"; // Username from bitcoin.conf rpcuser=
const RPC_PASS: &str = "password"; // Password from bitcoin.conf rpcpassword=

// Why regtest mode?
// - Mainnet: Real Bitcoin, expensive, slow (10min blocks)
// - Testnet: Fake Bitcoin, but still follows real network rules
// - Regtest: Complete control, instant blocks, perfect for learning

fn main() -> bitcoincore_rpc::Result<()> {
    // ═══════════════════════════════════════════════════════════════
    // SECTION 1: BLOCKCHAIN SETUP & CONNECTION
    // ═══════════════════════════════════════════════════════════════

    // Step 1: Connect to our local Bitcoin node
    // The RPC client is our interface to Bitcoin Core's functionality
    // This is like opening a connection to a database, but for blockchain operations
    let rpc = Client::new(
        RPC_URL,
        Auth::UserPass(RPC_USER.to_owned(), RPC_PASS.to_owned()),
    )?; // The ? operator propagates any connection errors up to main()'s Result return

    // Step 2: Get blockchain status information
    // In regtest mode, we start with 0 blocks and build our own private blockchain
    // This is useful for understanding the current state before we start operations
    let blockchain_info = rpc.get_blockchain_info()?;
    println!("Blockchain Info: {blockchain_info:#?}");
    // The #? formatting gives us pretty-printed debug output with proper indentation

    // ═══════════════════════════════════════════════════════════════
    // SECTION 2: WALLET MANAGEMENT
    // ═══════════════════════════════════════════════════════════════
    // Bitcoin Core can manage multiple wallets simultaneously
    // Each wallet has its own keys, addresses, and transaction history

    // Step 3: Set up two separate wallets for our simulation
    // We need two wallets to demonstrate a realistic transaction between parties:
    // - "Miner": Will mine blocks and earn Bitcoin rewards
    // - "Trader": Will receive Bitcoin from the Miner
    let wallets = rpc.list_wallets()?;
    for wallet in ["Miner", "Trader"] {
        if !wallets.contains(&wallet.to_string()) {
            // Create wallet parameters: (name, disable_private_keys, blank, passphrase, avoid_reuse)
            rpc.create_wallet(wallet, None, None, None, None)?;
        } else {
            // If wallet already exists, we need to load it into memory
            rpc.load_wallet(wallet)?;
        }
    }

    // ------------------------------------------
    // SUBSECTION: Wallet-Specific RPC Clients
    // ------------------------------------------
    // Why we need separate RPC clients for each wallet:
    // Bitcoin Core treats each wallet as a separate namespace
    // Operations like "get balance" or "send transaction" are wallet-specific
    // Using the same client would mix up wallet operations

    // Step 4: Create dedicated RPC connections for each wallet
    let miner_rpc = Client::new(
        &format!("{RPC_URL}/wallet/Miner"), // URL path includes wallet name
        Auth::UserPass(RPC_USER.to_owned(), RPC_PASS.to_owned()),
    )?;

    let trader_rpc = Client::new(
        &format!("{RPC_URL}/wallet/Trader"),
        Auth::UserPass(RPC_USER.to_owned(), RPC_PASS.to_owned()),
    )?;

    // ═══════════════════════════════════════════════════════════════
    // SECTION 3: ADDRESS GENERATION
    // ═══════════════════════════════════════════════════════════════
    // Bitcoin addresses are like account numbers, but with important differences:
    // - Each address should ideally be used only once (privacy)
    // - Addresses are derived from cryptographic keys
    // - You can generate unlimited addresses from one wallet

    // Step 5: Generate a Bitcoin address for mining rewards
    // The label "Mining Reward" helps us organize addresses in the wallet
    let miner_address = miner_rpc.get_new_address(Some("Mining Reward"), None)?;
    // Trust that Bitcoin Core gave us a valid address
    let miner_address = miner_address.assume_checked();
    // Note: In production code, you might want to validate addresses explicitly
    // let miner_address = miner_address
    //     .require_network(Network::Regtest)
    //     .map_err(|e| bitcoincore_rpc::Error::ReturnedError(e.to_string()))?;

    // ═══════════════════════════════════════════════════════════════
    // COINBASE MATURITY: Why We Need 101 Blocks Before Spending
    // ═══════════════════════════════════════════════════════════════
    //
    // CRITICAL BITCOIN CONCEPT: Coinbase Transaction Maturity
    //
    // When you mine a block, you get a "coinbase" reward (50 BTC in regtest)
    // BUT there's a consensus rule: You CANNOT spend this reward immediately!
    //
    // The Rule: Must wait for 100 MORE blocks after your block
    //
    // Why this rule exists - Attack Prevention:
    // 1. Alice mines Block 100, gets 50 BTC coinbase reward
    // 2. Alice immediately sends that 50 BTC to Bob
    // 3. Mallory creates a competing Block 100 without Alice's transaction
    // 4. If Mallory's chain becomes longer, Alice's block gets "orphaned"
    // 5. Alice's coinbase reward becomes invalid, but Bob already got the money!
    // 6. This would be a "double spend" - Alice spent money that doesn't exist
    //
    // The 100-block delay ensures that by the time coinbase rewards become spendable,
    // they're buried so deep that reorganizing them out
    // would require enormous computational effort (basically impossible).
    //
    // Timeline Example:
    // - Block 1: You mine it → Get 50 BTC reward → LOCKED (can't spend)
    // - Block 2-100: Other blocks get mined → Your reward still LOCKED
    // - Block 101: Your reward from Block 1 becomes SPENDABLE!
    //
    // So we need to mine at least 101 blocks before having spendable Bitcoin.

    println!("Mining blocks until we get a positive spendable balance...");

    // ═══════════════════════════════════════════════════════════════
    // SECTION 4: BLOCK MINING LOOP
    // ═══════════════════════════════════════════════════════════════

    // Step 7: Mine blocks until we have mature, spendable Bitcoin
    let (blocks_mined_count, spendable_balance) = {
        // Rust idiom explanation:
        // (1..) creates an infinite iterator: 1, 2, 3, 4, 5, ...
        // find_map() tries the closure on each number until it returns Some(value)
        // This is more elegant than a while loop with manual break conditions
        (1..).find_map(|count| {
            // Mine exactly 1 block and send the reward to our miner address
            // generate_to_address() creates a new block and assigns coinbase to our address
            // .ok()? converts Result to Option, continuing the loop on errors
            miner_rpc.generate_to_address(1, &miner_address).ok()?;

            // Check current spendable balance (only counts mature coins)
            // get_balance() with None parameters gets confirmed, spendable balance only
            let spendable_balance = miner_rpc.get_balance(None, None).ok()?;

            // Conditional return: if we finally have spendable money, return Some((count, balance))
            // If balance is still zero, return None to continue the loop
            #[allow(clippy::unnecessary_lazy_evaluations)] // skip tuple compute if balance < 0
            (spendable_balance > Amount::ZERO).then(|| (count, spendable_balance))
        })
    }
    // Fallback if something catastrophic happens (shouldn't in normal operation)
    .unwrap_or((0, Amount::ZERO));

    let (total_blocks_mined, spendable_balance_btc) =
        (blocks_mined_count, spendable_balance.to_btc());
    println!("Success! Mined {total_blocks_mined} blocks to get {spendable_balance_btc} BTC");
    println!("Our Miner wallet now has: {spendable_balance_btc} BTC available to spend");

    // ═══════════════════════════════════════════════════════════════
    // SECTION 5: TRANSACTION CREATION & BROADCAST
    // ═══════════════════════════════════════════════════════════════

    // Step 8: Set up the receiving wallet (Trader)
    let trader_address = trader_rpc.get_new_address(Some("Received"), None)?;
    let trader_address = trader_address.assume_checked();

    // Step 9: Create and broadcast a Bitcoin transaction
    // This is where Bitcoin's UTXO model becomes apparent
    // We're not "transferring money" - we're consuming previous outputs and creating new ones
    let amount_to_send = Amount::from_int_btc(20); // Send 20 BTC (out of our ~50+ BTC balance)

    // send_to_address() is a high-level RPC call that:
    // 1. Selects appropriate UTXOs (coins) from our wallet
    // 2. Creates a transaction consuming those UTXOs as inputs
    // 3. Creates two outputs: one to recipient, one back to us as "change"
    // 4. Calculates and includes appropriate mining fees
    // 5. Signs the transaction with our private keys
    // 6. Broadcasts it to the network (mempool)
    let transaction_id = miner_rpc.send_to_address(
        &trader_address, // Destination address
        amount_to_send,  // Amount to send
        None,            // Comment (stored locally, not on blockchain)
        None,            // Comment_to (stored locally, not on blockchain)
        None,            // Subtract fee from amount? (false = add fee on top)
        None,            // Replaceable? (RBF - Replace By Fee capability)
        None,            // Confirmation target (affects fee calculation)
        None,            // Estimate mode (affects fee calculation algorithm)
    )?;
    println!("Sent transaction with txid: {}", &transaction_id);

    // ═══════════════════════════════════════════════════════════════
    // SECTION 6: MEMPOOL ANALYSIS
    // ═══════════════════════════════════════════════════════════════
    // The mempool is Bitcoin's "waiting room" for unconfirmed transactions

    // Step 10: Examine our transaction in the mempool
    // Before transactions get included in blocks, they sit in the mempool
    // This is like a pending transaction list that miners choose from
    let mempool_entry = rpc.get_mempool_entry(&transaction_id)?;
    println!("Transaction in mempool: {mempool_entry:#?}");
    // This shows us fee rates, dependencies, and other mempool-specific data

    // ═══════════════════════════════════════════════════════════════
    // SECTION 7: TRANSACTION CONFIRMATION
    // ═══════════════════════════════════════════════════════════════

    // Step 11: Mine a block to confirm our transaction
    // This simulates what miners do: select transactions from mempool and include them in blocks
    let _confirmation_block_hash = miner_rpc.generate_to_address(1, &miner_address)?;
    println!("Mined 1 confirmation block - transaction is now confirmed!");
    // Once included in a block, the transaction moves from "pending" to "confirmed"

    // ═══════════════════════════════════════════════════════════════
    // SECTION 8: TRANSACTION FORENSICS & ANALYSIS
    // ═══════════════════════════════════════════════════════════════
    // This section demonstrates how to analyze Bitcoin transactions in detail
    // Understanding transaction structure is crucial for Bitcoin development

    // Step 12: Gather detailed transaction and blockchain data
    // get_raw_transaction() gives us the actual transaction data structure
    let raw_transaction = rpc.get_raw_transaction(&transaction_id, None)?;

    // Get current blockchain state for our report
    let latest_block_hash = rpc.get_best_block_hash()?; // Hash of most recent block
    let current_block_height = rpc.get_block_count()?; // Total number of blocks

    // Step 13: Initialize variables for transaction analysis
    // We'll extract all the key information from the raw transaction data
    // String::with_capacity(42) pre-allocates space for Bitcoin addresses (saves reallocations)
    // A Bitcoin bech32 regtest address typically has 42 characters
    let mut sender_address = String::with_capacity(42);
    let mut total_input_amount = 0.0;

    let mut recipient_address = String::with_capacity(42);
    let mut recipient_amount = 0.0;

    let mut change_return_address = String::with_capacity(42);
    let mut change_return_amount = 0.0;

    // ═══════════════════════════════════════════════════════════════
    // UTXO MODEL ANALYSIS: Understanding Bitcoin's Transaction Structure
    // ═══════════════════════════════════════════════════════════════
    //
    // Key Concept: Bitcoin uses UTXO (Unspent Transaction Output) model
    //
    // Unlike bank accounts (balance-based), Bitcoin transactions work like this:
    // 1. Inputs: Reference specific outputs from previous transactions
    // 2. Outputs: Create new "coins" that can be spent in future transactions
    // 3. Rule: Total inputs must equal or exceed total outputs + fees
    //
    // Example: If you have a 50 BTC UTXO and want to send 20 BTC:
    // - Input: Reference your 50 BTC UTXO
    // - Output 1: 20 BTC to recipient
    // - Output 2: 29.999 BTC back to you (change)
    // - Fee: 0.001 BTC (50 - 20 - 29.999 = 0.001)

    // Step 14: Analyze transaction inputs (where money came from)
    // Bitcoin transactions don't have "from" addresses directly
    // We need to look up the previous transaction to see who originally received this money
    if let Some(transaction_input) = raw_transaction.input.first() {
        // Each input references a specific output from a previous transaction
        // previous_output.txid = the transaction ID we're spending from
        // previous_output.vout = which output index from that transaction
        let previous_transaction =
            rpc.get_raw_transaction(&transaction_input.previous_output.txid, None)?;

        // Look at the specific output from that previous transaction that we're now spending
        let previous_output =
            &previous_transaction.output[transaction_input.previous_output.vout as usize];

        // Extract the value (how much Bitcoin was in that output)
        total_input_amount = previous_output.value.to_btc();

        // Decode the address from the script_pubkey (Bitcoin's locking script)
        // script_pubkey defines the conditions needed to spend this output
        sender_address = Address::from_script(&previous_output.script_pubkey, Network::Regtest)
            .map(|addr| addr.to_string())
            .unwrap_or_default(); // Use empty string if address decoding fails
    }

    // Step 15: Analyze transaction outputs (where money went)
    // A Bitcoin transaction typically has 2 outputs:
    // 1. Payment to recipient (what they requested)
    // 2. "Change" back to sender (like getting change from a $20 bill)
    let trader_address_str = trader_address.to_string();

    for transaction_output in &raw_transaction.output {
        // Try to decode the address from this output's script_pubkey
        let Ok(output_address) =
            Address::from_script(&transaction_output.script_pubkey, Network::Regtest)
        else {
            continue; // Skip outputs we can't decode (might be exotic script types)
        };

        let (address_str, output_amount) = (
            output_address.to_string(),
            transaction_output.value.to_btc(),
        );

        // Determine if this output went to our intended recipient or back to sender as change
        if address_str == trader_address_str {
            // This output went to our trader (the intended recipient)
            (recipient_address, recipient_amount) = (address_str, output_amount);
        } else {
            // This must be the "change" output going back to the sender's wallet
            (change_return_address, change_return_amount) = (address_str, output_amount);
        }
    }

    // ═══════════════════════════════════════════════════════════════
    // SECTION 9: FEE CALCULATION & VERIFICATION
    // ═══════════════════════════════════════════════════════════════

    // Step 16: Calculate transaction fees
    // Bitcoin transaction fees are calculated as: Total Inputs - Total Outputs
    // The "missing" money between inputs and outputs becomes the miner's fee
    // This incentivizes miners to include the transaction in their blocks
    let total_output_amount: f64 = raw_transaction
        .output
        .iter()
        .map(|out| out.value.to_btc())
        .sum();
    let mining_fees = total_input_amount - total_output_amount;

    // Fee verification: In a healthy transaction, fees should be positive but reasonable
    // Too low = transaction might not get confirmed quickly
    // Too high = you're overpaying miners

    // ═══════════════════════════════════════════════════════════════
    // SECTION 10: REPORT GENERATION
    // ═══════════════════════════════════════════════════════════════

    // Step 17: Write comprehensive transaction analysis to file
    // This creates a structured report of everything that happened
    // Format: One piece of information per line for easy parsing
    let mut output_file = File::create("../out.txt")?;

    // The write! macro is like println! but writes to a file instead of stdout
    // Each \n creates a new line in the output file
    // Line-by-line breakdown:
    // 1. Transaction ID (unique identifier for this transaction)
    // 2. Sender address (where the money originally came from)
    // 3. Total input amount (how much was available to spend)
    // 4. Recipient address (where the intended payment went)
    // 5. Recipient amount (how much the recipient got)
    // 6. Change address (where the leftover money went back)
    // 7. Change amount (how much went back as change)
    // 8. Mining fees (how much miners got for including this transaction)
    // 9. Current blockchain height (how many blocks exist now)
    // 10. Latest block hash (fingerprint of the most recent block)
    write!(
        output_file,
        "{transaction_id}\n{sender_address}\n{total_input_amount}\n{recipient_address}\n{recipient_amount}\n{change_return_address}\n{change_return_amount}\n{mining_fees:.2e}\n{current_block_height}\n{latest_block_hash}\n"
    )?;

    // ═══════════════════════════════════════════════════════════════
    // SECTION 11: SUMMARY OUTPUT
    // ═══════════════════════════════════════════════════════════════
    // Print human-readable summary of what we accomplished

    println!("Transaction details written to out.txt");
    println!(
        "Summary: Sent {recipient_amount} BTC from {sender_address} to {recipient_address}"
    );
    println!("Change: {change_return_amount} BTC returned to {change_return_address}");
    println!("Fees: {mining_fees:.8} BTC paid to miners");

    // Success! We've demonstrated:
    // ✓ Wallet creation and management
    // ✓ Block mining and coinbase maturity
    // ✓ Transaction creation and broadcasting
    // ✓ Mempool analysis
    // ✓ Transaction confirmation
    // ✓ Complete transaction forensics
    // ✓ UTXO model understanding
    // ✓ Fee calculation and verification

    Ok(())
}
