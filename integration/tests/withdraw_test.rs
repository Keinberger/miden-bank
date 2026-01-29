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
use miden_lib::note::utils::build_p2id_recipient;
use miden_objects::asset::{Asset, FungibleAsset};
use miden_testing::{Auth, MockChain};
use std::{path::Path, sync::Arc};

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

    let p2id_output_note_serial_num =
        Word::from([Felt::new(1), Felt::new(2), Felt::new(3), Felt::new(4)]);

    // Correct order of inputs
    let withdraw_request_note_inputs = vec![
        // WITHDRAW ASSET WORD (reverse order)
        Felt::new(withdraw_amount),
        Felt::new(0),
        faucet.id().suffix(),
        faucet.id().prefix().as_felt(),
        // P2ID OUTPUT NOTE SERIAL NUMBER WORD (correct order)
        Felt::new(1),
        Felt::new(2),
        Felt::new(3),
        Felt::new(4),
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

    // TODO: create expected P2ID output note
    let recipient = build_p2id_recipient(sender.id(), p2id_output_note_serial_num)?;
    let tag = NoteTag::LocalAny(3221225472);
    let aux = Felt::new(24);
    let p2id_output_note_asset = FungibleAsset::new(faucet.id(), withdraw_amount)?;
    let p2id_output_note_assets = NoteAssets::new(vec![p2id_output_note_asset.into()])?;
    let p2id_output_note_metadata = NoteMetadata::new(
        bank_account.id(),
        NoteType::Public,
        tag,
        NoteExecutionHint::none(),
        aux,
    )?;
    println!("Recipient raw: {:?}", recipient.digest()); // 0x792a35a4215690888984a2c0ccd8518d8270d6c815dc965771a88f0566a0311c
    println!("Recipient: {:?}", recipient.digest().to_hex()); // 0x792a35a4215690888984a2c0ccd8518d8270d6c815dc965771a88f0566a0311c
    let p2id_output_note = Note::new(
        p2id_output_note_assets,
        p2id_output_note_metadata,
        recipient,
    );

    println!(
        "faucet prefix: {:?}",
        faucet.id().prefix().as_felt().as_int()
    );
    println!("faucet suffix: {:?}", faucet.id().suffix().as_int());

    let withdraw_request_tx_context = mock_chain
        .build_tx_context(bank_account.id(), &[withdraw_request_note.id()], &[])?
        .extend_expected_output_notes(vec![OutputNote::Full(p2id_output_note.into())])
        .build()?;

    let executed_withdraw_request_transaction = withdraw_request_tx_context.execute().await?;

    bank_account.apply_delta(&executed_withdraw_request_transaction.account_delta())?;

    mock_chain.add_pending_executed_transaction(&executed_withdraw_request_transaction)?;
    mock_chain.prove_next_block()?;

    Ok(())
}
