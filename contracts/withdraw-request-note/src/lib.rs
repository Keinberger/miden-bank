// Do not link against libstd (i.e. anything defined in `std::`)
#![no_std]
#![feature(alloc_error_handler)]

use miden::*;

// Import the bank account's generated bindings
use crate::bindings::miden::bank_account::bank_account;

/// Withdraw Request Note Script
///
/// When consumed by the Bank account, this note transfers all its assets
/// to the bank and credits the depositor (note sender) with the deposited amount.
///
/// # Flow
/// 1. Note has fungible assets attached
/// 3. Note script reads the sender (depositor) and assets
/// 4. For each asset, calls `bank_account::withdraw(depositor, asset, amount)`
/// 5. Bank receives the asset & amount and updates the depositor's balance
/// 6. Bank creates a P2ID note to send the assets back to the depositor
///
/// # Note Inputs
/// None required - the depositor is automatically the note's sender.
#[note_script]
fn run(_arg: Word) {
    // The depositor is whoever created/sent this note
    let depositor = active_note::get_sender();

    // Get the amount (asset) to withdraw
    let inputs = active_note::get_inputs();

    // Reversed order asset
    let withdraw_asset = Asset::new(Word::from([inputs[0], inputs[1], inputs[2], inputs[3]]));

    // Correct order of serial number
    let serial_num = Word::from([inputs[4], inputs[5], inputs[6], inputs[7]]);

    // Call the bank account to withdraw the assets
    bank_account::withdraw(depositor, withdraw_asset, serial_num);
}
