#![allow(unused)]
use bitcoin::hex::DisplayHex;
use bitcoincore_rpc::bitcoin::Amount;
use bitcoincore_rpc::{Auth, Client, RpcApi};
use serde::Deserialize;
use serde_json::json;
use std::fs::File;
use std::io::Write;
use std::str::FromStr;

// Node access params
const RPC_URL: &str = "http://127.0.0.1:18443"; // Default regtest RPC port
const RPC_USER: &str = "alice";
const RPC_PASS: &str = "password";

// You can use calls not provided in RPC lib API using the generic `call` function.
// An example of using the `send` RPC call, which doesn't have exposed API.
// You can also use serde_json `Deserialize` derivation to capture the returned json result.
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

/// Check if a wallet is already loaded
fn is_wallet_loaded(rpc: &Client, wallet_name: &str) -> bool {
    match rpc.call::<Vec<String>>("listwallets", &[]) {
        Ok(wallets) => wallets.contains(&wallet_name.to_string()),
        Err(_) => false,
    }
}

/// Create or load a wallet with the given name
fn create_or_load_wallet(rpc: &Client, wallet_name: &str) -> bitcoincore_rpc::Result<bool> {
    // First check if the wallet is already loaded
    if is_wallet_loaded(rpc, wallet_name) {
        println!("Wallet '{wallet_name}' is already loaded");
        return Ok(false);
    }

    // Try to create the wallet first (this handles most cases cleanly)
    match rpc.create_wallet(wallet_name, None, None, None, None) {
        Ok(_) => {
            println!("Wallet '{wallet_name}' created successfully");
            Ok(true) // Wallet was created
        }
        Err(create_err) => {
            let create_error_msg = create_err.to_string();
            // If creation fails due to existing wallet, try to load it
            if create_error_msg.contains("Database already exists")
                || create_error_msg.contains("already exists")
            {
                println!("Wallet '{wallet_name}' already exists, trying to load it");
                match rpc.load_wallet(wallet_name) {
                    Ok(_) => {
                        println!("Wallet '{wallet_name}' loaded successfully");
                        Ok(false)
                    }
                    Err(load_err) => {
                        println!("Warning: Could not load wallet '{wallet_name}': {load_err}");
                        // Continue anyway, the wallet might be usable
                        Ok(false)
                    }
                }
            } else {
                Err(create_err)
            }
        }
    }
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

    // Create/Load the wallets, named 'Miner' and 'Trader'. Have logic to optionally create/load them if they do not exist or not loaded already.
    println!("\n=== Setting up wallets ===");

    let miner_created = create_or_load_wallet(&rpc, "Miner")?;
    let trader_created = create_or_load_wallet(&rpc, "Trader")?;

    println!("Miner wallet created: {miner_created}");
    println!("Trader wallet created: {trader_created}");

    // Create wallet-specific RPC clients
    let miner_rpc = Client::new(
        &format!("{RPC_URL}/wallet/Miner"),
        Auth::UserPass(RPC_USER.to_owned(), RPC_PASS.to_owned()),
    )?;

    let trader_rpc = Client::new(
        &format!("{RPC_URL}/wallet/Trader"),
        Auth::UserPass(RPC_USER.to_owned(), RPC_PASS.to_owned()),
    )?;

    // Generate spendable balances in the Miner wallet. How many blocks needs to be mined?
    println!("\n=== Generating mining rewards ===");

    let miner_address = miner_rpc.get_new_address(Some("Mining Reward"), None)?;
    println!("Generated mining reward address: {miner_address:?}");

    // Convert address to string format for RPC calls
    let miner_address_str = miner_address.assume_checked().to_string();

    let mut blocks_mined = 0;
    let mut miner_balance = Amount::ZERO;

    while miner_balance <= Amount::ZERO {
        blocks_mined += 1;
        // println!(
        //     "Mining block {} to address {}",
        //     blocks_mined, miner_address_str
        // );

        let block_hashes = miner_rpc
            .call::<Vec<String>>("generatetoaddress", &[json!(1), json!(miner_address_str)])?;
        println!("Mined block: {block_hashes:?}");

        // Coinbase rewards require 100 block confirmations before becoming spendable to prevent issues from chain reorganizations.
        // This is why we need to mine 100 blocks before the miner balance is greater than 0.
        miner_balance = miner_rpc.get_balance(None, None)?;
        println!(
            "Miner wallet balance after {} blocks: {} BTC",
            blocks_mined,
            miner_balance.to_btc()
        );
    }
    // Load Trader wallet and generate a new address
    println!("\n=== Setting up Trader wallet ===");

    // The Trader wallet should already be loaded after creation/loading

    let trader_address = trader_rpc.get_new_address(Some("Received"), None)?;
    println!("Generated Trader receiving address: {trader_address:?}");

    // Convert trader address to string format for RPC calls
    let trader_address_str = trader_address.assume_checked().to_string();

    let trader_balance = trader_rpc.get_balance(None, None)?;
    println!("Trader wallet balance: {} BTC", trader_balance.to_btc());

    // Send 20 BTC from Miner to Trader
    println!("\n=== Sending 20 BTC from Miner to Trader ===");

    let miner_balance_before = miner_rpc.get_balance(None, None)?;
    println!(
        "Miner balance before sending: {} BTC",
        miner_balance_before.to_btc()
    );

    let amount_to_send = Amount::from_btc(20.0)?;
    println!(
        "Sending {} BTC from Miner to Trader at address: {}",
        amount_to_send.to_btc(),
        trader_address_str
    );

    let txid = miner_rpc.call::<String>(
        "sendtoaddress",
        &[
            json!(trader_address_str),
            json!(amount_to_send.to_btc()),
            json!(""),
            json!(""),
            json!(false),
            json!(false),
            json!(null),
            json!(null),
            json!(null),
            json!(null),
        ],
    )?;
    println!("Transaction sent! TXID: {txid}");

    let txid_parsed = bitcoincore_rpc::bitcoin::Txid::from_str(&txid).unwrap();

    // Check transaction in mempool
    println!("\n=== Checking transaction in mempool ===");

    let mempool_entry = miner_rpc.get_mempool_entry(&txid_parsed)?;
    println!("Transaction found in mempool:");
    println!("  Size: {} bytes", mempool_entry.vsize);
    println!("  Fee: {} BTC", mempool_entry.fees.base.to_btc());
    println!("  Time: {}", mempool_entry.time);
    println!("  Height: {}", mempool_entry.height);

    // Mine 1 block to confirm the transaction
    println!("\n=== Mining 1 block to confirm the transaction ===");

    let confirmation_block_hashes = miner_rpc
        .call::<Vec<String>>("generatetoaddress", &[json!(1), json!(miner_address_str)])?;
    println!("Mined confirmation block: {confirmation_block_hashes:?}");

    let confirmation_block_hash = &confirmation_block_hashes[0];
    println!("Transaction confirmed in block: {confirmation_block_hash}");

    let block_hash_parsed =
        bitcoincore_rpc::bitcoin::BlockHash::from_str(confirmation_block_hash).unwrap();

    // Get the block height where the transaction was confirmed
    let blockchain_info = rpc.get_blockchain_info()?;
    let confirmation_block_height = blockchain_info.blocks;
    println!("Transaction confirmed at block height: {confirmation_block_height}");

    // Verify the transaction is now confirmed
    let confirmed_tx = miner_rpc.get_raw_transaction(&txid_parsed, Some(&block_hash_parsed))?;
    println!("Transaction is now confirmed!");
    println!("Confirmed transaction details:");
    println!("  Block hash: {confirmation_block_hash}");
    println!("  Block height: {confirmation_block_height}");
    println!("  Transaction ID: {txid}");

    let final_miner_balance = miner_rpc.get_balance(None, None)?;
    println!("Final Miner balance: {} BTC", final_miner_balance.to_btc());

    let final_trader_balance = trader_rpc.get_balance(None, None)?;
    println!(
        "Final Trader balance: {} BTC",
        final_trader_balance.to_btc()
    );

    // Write the data to ../out.txt in the specified format given in readme.md
    println!("\n=== Extracting transaction details and writing to out.txt ===");

    // Get the confirmed transaction details to extract all required information
    let confirmed_tx = miner_rpc.get_raw_transaction(&txid_parsed, Some(&block_hash_parsed))?;

    // Extract transaction details
    let txid_str = txid.to_string();

    let miner_input_address = miner_address_str.clone();
    let miner_input_amount = "50.0";

    // Get actual output addresses by calling get_decoded_transaction
    let decoded_tx = miner_rpc.call::<serde_json::Value>(
        "getrawtransaction",
        &[json!(txid_str), json!(true), json!(confirmation_block_hash)],
    )?;

    let vouts = decoded_tx["vout"].as_array().unwrap();

    // Find the trader output (20 BTC) and miner change output by amount
    let mut trader_output_address = trader_address_str.clone();
    let mut miner_change_address = miner_address_str.clone();
    let mut miner_change_amount = "0.0".to_string();

    for vout in vouts {
        let value = vout["value"].as_f64().unwrap_or(0.0);
        if let Some(address) = vout["scriptPubKey"]["address"].as_str() {
            if (value - 20.0).abs() < 0.0001 {
                // This is the trader output (exactly 20 BTC)
                trader_output_address = address.to_string();
            } else if value > 0.0 && (value - 20.0).abs() >= 0.0001 {
                // This is the change output (not exactly 20 BTC)
                miner_change_address = address.to_string();
                miner_change_amount = format!("{value:.8}");
            }
        }
    }

    let trader_output_amount = "20.0";

    // Get transaction fees
    let fee_btc = mempool_entry.fees.base.to_btc();
    let transaction_fees = format!("{fee_btc:.8}");

    // Get block height and hash
    let block_height = confirmation_block_height.to_string();
    let block_hash = confirmation_block_hash.to_string();

    // Write to out.txt file in the correct location (parent directory)
    let mut output_file = File::create("../out.txt")?;
    writeln!(output_file, "{txid_str}")?;
    writeln!(output_file, "{miner_input_address}")?;
    writeln!(output_file, "{miner_input_amount}")?;
    writeln!(output_file, "{trader_output_address}")?;
    writeln!(output_file, "{trader_output_amount}")?;
    writeln!(output_file, "{miner_change_address}")?;
    writeln!(output_file, "{miner_change_amount}")?;
    writeln!(output_file, "{transaction_fees}")?;
    writeln!(output_file, "{block_height}")?;
    writeln!(output_file, "{block_hash}")?;

    Ok(())
}
