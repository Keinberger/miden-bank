// Do not link against libstd (i.e. anything defined in `std::`)
#![no_std]
#![feature(alloc_error_handler)]

use miden::*;

// Import the bank account's generated bindings
use crate::bindings::miden::bank_account::bank_account;

/// Withdraw Request Note Script
///
/// When consumed by the Bank account, this note requests a withdrawal and
/// the bank creates a P2ID note to send assets back to the depositor.
///
/// # Flow
/// 1. Note is created by a depositor specifying the withdrawal details
/// 2. Bank account consumes this note
/// 3. Note script reads the sender (depositor) and inputs
/// 4. Calls `bank_account::withdraw(depositor, asset, serial_num, tag)`
/// 5. Bank updates the depositor's balance
/// 6. Bank creates a P2ID note with the specified tag to send assets back
///
/// # Note Inputs (8 Felts = 2 Words)
/// [0]: withdraw amount
/// [1]: 0 (asset padding)
/// [2]: faucet suffix
/// [3]: faucet prefix
/// [4]: serial_num[0]
/// [5]: serial_num[1]
/// [6]: serial_num[2]
/// [7]: tag (P2ID note tag as Felt)
#[note_script]
fn run(_arg: Word) {
    // The depositor is whoever created/sent this note
    let depositor = active_note::get_sender();

    // Get the inputs
    let inputs = active_note::get_inputs();

    // Asset: [amount, 0, faucet_suffix, faucet_prefix]
    let withdraw_asset = Asset::new(Word::from([inputs[0], inputs[1], inputs[2], inputs[3]]));

    // Serial number: 3 Felts + padding (4th element is 0)
    let serial_num = Word::from([inputs[4], inputs[5], inputs[6], felt!(0)]);

    // Tag for the P2ID output note
    let tag = inputs[7];

    // Call the bank account to withdraw the assets
    bank_account::withdraw(depositor, withdraw_asset, serial_num, tag);
}
