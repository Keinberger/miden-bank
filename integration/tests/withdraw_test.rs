use integration::helpers::{
    build_project_in_dir, create_testing_account_from_package, create_testing_note_from_package,
    AccountCreationConfig, NoteCreationConfig,
};

use miden_client::{
    account::StorageMap,
    note::{Note, NoteAssets, NoteExecutionHint, NoteMetadata, NoteTag, NoteType},
    transaction::OutputNote,
    Felt, Word,
};
use miden_core::crypto::hash::Rpo256;
use miden_lib::note::utils::build_p2id_recipient;
use miden_objects::{
    account::AccountId,
    asset::{Asset, FungibleAsset},
};
use miden_testing::{Auth, MockChain};
use std::{path::Path, sync::Arc};

/// Compute a P2ID note tag for a local account.
///
/// This mimics `NoteTag::from_account_id()` from miden-base which is not available
/// in miden-objects 0.12. The algorithm extracts the top 14 bits of the account ID
/// prefix and combines them with the LocalAny prefix (0xC0000000).
///
/// # Arguments
/// * `account_id` - The target account ID that will consume the P2ID note
///
/// # Returns
/// A NoteTag::LocalAny with account ID bits embedded for routing
fn compute_p2id_tag_for_local_account(account_id: AccountId) -> NoteTag {
    const LOCAL_ANY_PREFIX: u32 = 0xC000_0000; // 0b11 prefix for LocalAny
    const TAG_BITS: u8 = 14; // Default tag length for local accounts

    // Get the account ID prefix as u64
    let prefix_u64 = account_id.prefix().as_u64();

    // Right shift by 34 to get top 30 bits of the 64-bit prefix, cast to u32
    let shifted = (prefix_u64 >> 34) as u32;

    // Mask to keep only top TAG_BITS bits in the 30-bit space
    // This creates a mask like 0x3FFF0000 for 14 bits
    let mask = u32::MAX << (30 - TAG_BITS);
    let account_bits = shifted & mask;

    // Combine with LocalAny prefix
    let tag_value = LOCAL_ANY_PREFIX | account_bits;

    NoteTag::LocalAny(tag_value)
}

#[tokio::test]
async fn withdraw_test() -> anyhow::Result<()> {
    // *********************************************************************************
    // SETUP
    // *********************************************************************************

    // Test that after executing the deposit note, the depositor's balance is updated
    let mut builder = MockChain::builder();

    // Define the deposit amount
    let deposit_amount: u64 = 1000;

    // Create a faucet to mint test assets
    let faucet =
        builder.add_existing_basic_faucet(Auth::BasicAuth, "TEST", deposit_amount, Some(10))?;

    // Create note sender account (the depositor)
    let sender = builder.add_existing_wallet_with_assets(
        Auth::BasicAuth,
        [FungibleAsset::new(faucet.id(), deposit_amount)?.into()],
    )?;

    // Build contracts
    let bank_package = Arc::new(build_project_in_dir(
        Path::new("../contracts/bank-account"),
        true,
    )?);
    let deposit_note_package = Arc::new(build_project_in_dir(
        Path::new("../contracts/deposit-note"),
        true,
    )?);

    // Create the bank account with empty storage
    let bank_cfg = AccountCreationConfig {
        storage_slots: vec![miden_client::account::StorageSlot::Map(
            StorageMap::with_entries([])?,
        )],
        ..Default::default()
    };

    let mut bank_account =
        create_testing_account_from_package(bank_package.clone(), bank_cfg).await?;

    // *********************************************************************************
    // STEP 1: CRAFT DEPOSIT NOTE
    // *********************************************************************************

    // Create a fungible asset to deposit
    let fungible_asset = FungibleAsset::new(faucet.id(), deposit_amount)?;
    let note_assets = NoteAssets::new(vec![Asset::Fungible(fungible_asset)])?;

    // Create the deposit note with assets attached
    // The sender becomes the depositor
    let deposit_note = create_testing_note_from_package(
        deposit_note_package.clone(),
        sender.id(),
        NoteCreationConfig {
            assets: note_assets,
            ..Default::default()
        },
    )?;

    // Add bank account and deposit note to mockchain
    builder.add_account(bank_account.clone())?;
    builder.add_output_note(OutputNote::Full(deposit_note.clone().into()));

    // *********************************************************************************
    // STEP 2: CRAFT WITHDRAW REQUEST NOTE
    // *********************************************************************************

    let withdraw_amount = deposit_amount / 2;

    // Compute proper P2ID tag for the sender (depositor) who will consume the output note
    let p2id_tag = compute_p2id_tag_for_local_account(sender.id());
    let p2id_tag_u32 = match p2id_tag {
        NoteTag::LocalAny(v) => v,
        _ => panic!("Expected LocalAny tag"),
    };
    let p2id_tag_felt = Felt::new(p2id_tag_u32 as u64);

    println!("Computed P2ID tag for sender: 0x{:08X}", p2id_tag_u32);

    // Tag data padded to 4 Felts (1 Word)
    let tag_data = vec![p2id_tag_felt, Felt::new(0), Felt::new(0), Felt::new(0)];

    // Random serial number - MUST be unique per note
    // In production, this would be generated randomly. For testing, we use fixed values.
    let p2id_output_note_serial_num = Word::from([
        Felt::new(0x1234567890abcdef),
        Felt::new(0xfedcba0987654321),
        Felt::new(0xdeadbeefcafebabe),
        Felt::new(0x0123456789abcdef),
    ]);

    // Compute the commitment = hash(tag_data)
    // This is used as the second key in the two-level advice lookup
    let commitment_digest = Rpo256::hash_elements(&tag_data);
    let commitment_word = Word::from([
        commitment_digest[0],
        commitment_digest[1],
        commitment_digest[2],
        commitment_digest[3],
    ]);

    println!(
        "Serial num (random): {:?}",
        p2id_output_note_serial_num
    );
    println!(
        "Commitment (hash of tag data): {:?}",
        commitment_word
    );

    // Note inputs layout:
    // [0-3]: withdraw asset (amount, 0, faucet_suffix, faucet_prefix)
    // [4-7]: serial_num (random/unique per note)
    // [8-11]: commitment (hash of [tag, 0, 0, 0]) - used as key for advice lookup
    // Tag is loaded from advice provider using commitment as key
    let withdraw_request_note_inputs = vec![
        // WITHDRAW ASSET WORD
        Felt::new(withdraw_amount),
        Felt::new(0),
        faucet.id().suffix(),
        faucet.id().prefix().as_felt(),
        // P2ID OUTPUT NOTE SERIAL NUMBER (random, unique per note)
        p2id_output_note_serial_num[0],
        p2id_output_note_serial_num[1],
        p2id_output_note_serial_num[2],
        p2id_output_note_serial_num[3],
        // COMMITMENT (hash of [tag, 0, 0, 0])
        commitment_word[0],
        commitment_word[1],
        commitment_word[2],
        commitment_word[3],
    ];

    // Advice map entry:
    // commitment -> [tag, 0, 0, 0]
    // The note script uses this commitment (passed in inputs) as the key
    // adv_load_preimage verifies that hash(tag_data) == commitment
    let advice_map_entries: Vec<(Word, Vec<Felt>)> = vec![
        (commitment_word, tag_data),
    ];

    let withdraw_request_note_package = Arc::new(build_project_in_dir(
        Path::new("../contracts/withdraw-request-note"),
        true,
    )?);

    let withdraw_request_note = create_testing_note_from_package(
        withdraw_request_note_package.clone(),
        sender.id(),
        NoteCreationConfig {
            inputs: withdraw_request_note_inputs,
            ..Default::default()
        },
    )?;

    builder.add_output_note(OutputNote::Full(withdraw_request_note.clone().into()));

    // *********************************************************************************
    // STEP 3: MAKE DEPOSIT
    // *********************************************************************************

    // Build the mock chain
    let mut mock_chain = builder.build()?;

    // Build the transaction context where bank consumes the deposit note
    let deposit_tx_context = mock_chain
        .build_tx_context(bank_account.id(), &[deposit_note.id()], &[])?
        .build()?;

    // Execute the transaction
    let executed_deposit_transaction = deposit_tx_context.execute().await?;

    // Apply the account delta to the bank account
    bank_account.apply_delta(&executed_deposit_transaction.account_delta())?;

    // Add the executed transaction to the mockchain and prove
    mock_chain.add_pending_executed_transaction(&executed_deposit_transaction)?;
    mock_chain.prove_next_block()?;

    println!("Bank deposit successful");

    // *********************************************************************************
    // STEP 4: MAKE WITHDRAW
    // *********************************************************************************

    // Create expected P2ID output note with the computed tag
    let recipient = build_p2id_recipient(sender.id(), p2id_output_note_serial_num)?;
    let aux = Felt::new(0); // aux is 0 in the bank account code
    let p2id_output_note_asset = FungibleAsset::new(faucet.id(), withdraw_amount)?;
    let p2id_output_note_assets = NoteAssets::new(vec![p2id_output_note_asset.into()])?;
    let p2id_output_note_metadata = NoteMetadata::new(
        bank_account.id(),
        NoteType::Public,
        p2id_tag, // Use the computed tag
        NoteExecutionHint::none(),
        aux,
    )?;

    println!("Recipient digest: {:?}", recipient.digest().to_hex());

    let p2id_output_note = Note::new(
        p2id_output_note_assets,
        p2id_output_note_metadata,
        recipient,
    );

    let withdraw_request_tx_context = mock_chain
        .build_tx_context(bank_account.id(), &[withdraw_request_note.id()], &[])?
        .extend_advice_map(advice_map_entries) // Inject the tag via advice provider
        .extend_expected_output_notes(vec![OutputNote::Full(p2id_output_note.into())])
        .build()?;

    let executed_withdraw_request_transaction = withdraw_request_tx_context.execute().await?;

    bank_account.apply_delta(&executed_withdraw_request_transaction.account_delta())?;

    mock_chain.add_pending_executed_transaction(&executed_withdraw_request_transaction)?;
    mock_chain.prove_next_block()?;

    println!("Withdraw test passed!");

    Ok(())
}
