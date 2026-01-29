use integration::helpers::{
    build_project_in_dir, create_testing_account_from_package, create_testing_note_from_package,
    AccountCreationConfig, NoteCreationConfig,
};

use miden_client::{account::StorageMap, note::NoteAssets, transaction::OutputNote, Felt, Word};
use miden_objects::asset::{Asset, FungibleAsset};
use miden_testing::{Auth, MockChain};
use std::{path::Path, sync::Arc};

#[tokio::test]
async fn deposit_test() -> anyhow::Result<()> {
    // Test that after executing the deposit note, the depositor's balance is updated
    let mut builder = MockChain::builder();

    // Create a faucet to mint test assets
    let faucet = builder.add_existing_basic_faucet(Auth::BasicAuth, "TEST", 1000, Some(10))?;

    // Create note sender account (the depositor)
    let sender = builder.add_existing_wallet_with_assets(
        Auth::BasicAuth,
        [FungibleAsset::new(faucet.id(), 100)?.into()],
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

    // Create a fungible asset to deposit
    let deposit_amount: u64 = 1000;
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

    // Build the mock chain
    let mut mock_chain = builder.build()?;

    // Build the transaction context where bank consumes the deposit note
    let tx_context = mock_chain
        .build_tx_context(bank_account.id(), &[deposit_note.id()], &[])?
        .build()?;

    // Execute the transaction
    let executed_transaction = tx_context.execute().await?;

    // Apply the account delta to the bank account
    bank_account.apply_delta(&executed_transaction.account_delta())?;

    // Add the executed transaction to the mockchain and prove
    mock_chain.add_pending_executed_transaction(&executed_transaction)?;
    mock_chain.prove_next_block()?;

    // Create the key for the depositor (sender) in the storage map
    // Key format: [prefix, suffix, 0, 0]
    let depositor_key = Word::from([
        sender.id().prefix().as_felt(),
        sender.id().suffix(),
        faucet.id().prefix().as_felt(),
        faucet.id().suffix(),
    ]);

    // Get the depositor's balance from the bank's storage
    let balance = bank_account.storage().get_map_item(0, depositor_key)?;

    // The balance should be stored as [amount, 0, 0, 0] in the Word
    // But since we store just the Felt, check the first element
    let expected_balance = Word::from([
        Felt::new(0),
        Felt::new(0),
        Felt::new(0),
        Felt::new(deposit_amount),
    ]);

    assert_eq!(
        balance, expected_balance,
        "Depositor balance should equal the deposited amount"
    );

    println!("Deposit test passed! Deposited {} tokens", deposit_amount);
    Ok(())
}
