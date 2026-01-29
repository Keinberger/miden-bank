// Do not link against libstd (i.e. anything defined in `std::`)
#![no_std]
#![feature(alloc_error_handler)]

use miden::*;
use miden::intrinsics::advice::adv_push_mapvaln;

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
/// 4. Note script loads tag from advice provider using commitment
/// 5. Calls `bank_account::withdraw(depositor, asset, serial_num, tag)`
/// 6. Bank updates the depositor's balance
/// 7. Bank creates a P2ID note with the specified tag to send assets back
///
/// # Note Inputs (12 Felts = 3 Words)
/// [0]: withdraw amount
/// [1]: 0 (asset padding)
/// [2]: faucet suffix
/// [3]: faucet prefix
/// [4-7]: serial_num (full 4 Felts, random/unique per note)
/// [8-11]: commitment (hash of [tag, 0, 0, 0])
///
/// # Advice Provider
/// Key: commitment (hash of [tag, 0, 0, 0])
/// Value: [tag, 0, 0, 0]
#[note_script]
fn run(_arg: Word) {
    // The depositor is whoever created/sent this note
    let depositor = active_note::get_sender();

    // Get the inputs
    let inputs = active_note::get_inputs();

    // Asset: [amount, 0, faucet_suffix, faucet_prefix]
    let withdraw_asset = Asset::new(Word::from([inputs[0], inputs[1], inputs[2], inputs[3]]));

    // Serial number: full 4 Felts (random/unique per note)
    let serial_num = Word::from([inputs[4], inputs[5], inputs[6], inputs[7]]);

    // Commitment: hash of [tag, 0, 0, 0] - used as key for advice lookup
    let commitment = Word::from([inputs[8], inputs[9], inputs[10], inputs[11]]);

    // Load tag from advice provider using commitment as key.
    // The advice map contains: commitment -> [tag, 0, 0, 0]
    // where commitment = hash([tag, 0, 0, 0])
    //
    // adv_push_mapvaln pushes the values onto the advice stack
    // adv_load_preimage pops them and verifies hash matches commitment
    let _num_felts = adv_push_mapvaln(commitment);

    // Load with commitment verification
    let tag_data = adv_load_preimage(felt!(1), commitment);
    let tag = tag_data[0];

    // Call the bank account to withdraw the assets
    bank_account::withdraw(depositor, withdraw_asset, serial_num, tag);
}
