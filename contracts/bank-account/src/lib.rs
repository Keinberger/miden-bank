// Do not link against libstd (i.e. anything defined in `std::`)
#![no_std]
#![feature(alloc_error_handler)]

#[macro_use]
extern crate alloc;

use miden::*;

/// Maximum allowed deposit amount per transaction.
///
/// This limit provides a safety constraint for the banking system.
///
/// Value: 1,000,000 tokens (arbitrary limit for demonstration)
///
/// # Implementation Notes
/// In Miden Rust contracts, constants are defined using standard Rust `const` syntax.
/// The value is a u64 which can be compared against a Felt's underlying representation
/// using the `as_u64()` method.
///
/// # Error Handling
/// When this limit is exceeded, the contract uses `assert!()` to fail the transaction.
/// In the Miden VM, a failed assertion means the proof cannot be generated,
/// effectively rejecting the transaction at the proving stage.
const MAX_DEPOSIT_AMOUNT: u64 = 1_000_000;

/// Bank account component that tracks depositor balances.
///
/// Users deposit assets via deposit notes, and the bank tracks
/// each depositor's balance in a storage map keyed by their AccountId.
///
/// The bank must be initialized before deposits are accepted. This is done
/// via a transaction script that calls the `initialize()` method.
#[component]
struct Bank {
    /// Tracks whether the bank has been initialized (deposits enabled).
    /// Word layout: [is_initialized (0 or 1), 0, 0, 0]
    /// Must be set to 1 via `initialize()` before deposits are accepted.
    #[storage(slot(0), description = "initialized")]
    initialized: Value,

    /// Maps depositor AccountId -> balance (as Felt)
    /// Key is derived from AccountId: [prefix, suffix, asset_prefix, asset_suffix]
    #[storage(slot(1), description = "balances")]
    balances: StorageMap,
}

#[component]
impl Bank {
    /// Initialize the bank account, enabling deposits.
    ///
    /// This function should be called via a transaction script by the account owner.
    /// Once initialized, the bank can accept deposits. This also serves to "deploy"
    /// the account on-chain (accounts are only visible after their first state change).
    ///
    /// # Panics
    /// Panics if the bank is already initialized.
    pub fn initialize(&mut self) {
        // Check not already initialized
        let current: Word = self.initialized.read();
        assert!(
            current[0].as_u64() == 0,
            "Bank already initialized"
        );

        // Set initialized flag to 1
        let initialized_word = Word::from([felt!(1), felt!(0), felt!(0), felt!(0)]);
        self.initialized.write(initialized_word);
    }

    /// Check that the bank is initialized.
    ///
    /// This internal function is called at the start of operations that require
    /// the bank to be initialized (e.g., deposits).
    ///
    /// # Panics
    /// Panics if the bank has not been initialized.
    fn require_initialized(&self) {
        let current: Word = self.initialized.read();
        assert!(
            current[0].as_u64() == 1,
            "Bank not initialized - deposits not enabled"
        );
    }

    /// Returns the P2ID note script root digest.
    ///
    /// This is a constant value derived from the standard P2ID note script in miden-lib.
    /// The digest is the MAST root of the compiled P2ID note script.
    ///
    /// Note: This value is version-specific to miden-lib. If the P2ID script changes
    /// in a future version, this digest will need to be updated.
    fn p2id_note_root() -> Digest {
        Digest::from_word(Word::new([
            Felt::from_u64_unchecked(15783632360113277539),
            Felt::from_u64_unchecked(7403765918285273520),
            Felt::from_u64_unchecked(15691985194755641846),
            Felt::from_u64_unchecked(10399643920503194563),
        ]))
    }

    /// Get the balance for a depositor.
    ///
    /// # Arguments
    /// * `depositor` - The AccountId to query the balance for
    ///
    /// # Returns
    /// The depositor's current balance as a Felt
    pub fn get_balance(&self, depositor: AccountId) -> Felt {
        let key = Word::from([depositor.prefix, depositor.suffix, felt!(0), felt!(0)]);
        self.balances.get(&key)
    }

    /// Deposit an asset into the bank for a specific depositor.
    ///
    /// The asset is added to the bank's vault and the depositor's
    /// balance is updated in the mapping.
    ///
    /// # Arguments
    /// * `depositor` - The AccountId of the user making the deposit
    /// * `asset` - The fungible asset being deposited
    ///
    /// # Panics
    /// Panics if the deposit amount exceeds `MAX_DEPOSIT_AMOUNT`.
    /// Panics if the bank has not been initialized.
    pub fn deposit(&mut self, depositor: AccountId, deposit_asset: Asset) {
        // ========================================================================
        // CONSTRAINT: Bank must be initialized
        // ========================================================================
        self.require_initialized();

        // Extract the fungible amount from the asset
        // Asset inner layout for fungible: [amount, 0, faucet_suffix, faucet_prefix]
        // Asset.inner is a Word field, access it directly
        let deposit_amount = deposit_asset.inner[0];

        // ========================================================================
        // CONSTRAINT: Maximum deposit amount check
        // ========================================================================
        // Enforce that the deposit amount does not exceed the maximum allowed.
        // This check uses Rust's assert! macro which will cause the transaction
        // to fail if the condition is not met. In the Miden VM, a failed assertion
        // means the proof cannot be generated, effectively rejecting the transaction.
        //
        // We compare using as_u64() to get the underlying u64 value from the Felt.
        assert!(
            deposit_amount.as_u64() <= MAX_DEPOSIT_AMOUNT,
            "Deposit amount exceeds maximum allowed"
        );

        // Create key from depositor's AccountId and asset faucet ID
        // This allows tracking balances per depositor per asset type
        let key = Word::from([
            depositor.prefix,
            depositor.suffix,
            deposit_asset.inner[3], // asset prefix (faucet)
            deposit_asset.inner[2], // asset suffix (faucet)
        ]);

        // Update balance: current + deposit_amount
        let current_balance: Felt = self.balances.get(&key);
        let new_balance = current_balance + deposit_amount;
        self.balances.set(key, new_balance);

        // Add asset to the bank's vault
        native_account::add_asset(deposit_asset);
    }

    /// Withdraw assets back to the depositor.
    ///
    /// Creates a P2ID note that sends the requested asset to the depositor's account.
    ///
    /// # Arguments
    /// * `depositor` - The AccountId of the user withdrawing
    /// * `withdraw_asset` - The fungible asset to withdraw
    /// * `serial_num` - Unique serial number for the P2ID output note
    /// * `tag` - The note tag for the P2ID output note (allows caller to specify routing)
    /// * `aux` - Auxiliary data for the note (application-specific, typically 0)
    /// * `note_type` - Note type: 1 = Public (stored on-chain), 2 = Private (off-chain)
    pub fn withdraw(
        &mut self,
        depositor: AccountId,
        withdraw_asset: Asset,
        serial_num: Word,
        tag: Felt,
        aux: Felt,
        note_type: Felt,
    ) {
        // Extract the fungible amount from the asset
        let withdraw_amount = withdraw_asset.inner[0];

        // Create key from depositor's AccountId and asset faucet ID
        let key = Word::from([
            depositor.prefix,
            depositor.suffix,
            withdraw_asset.inner[3], // asset prefix (faucet)
            withdraw_asset.inner[2], // asset suffix (faucet)
        ]);

        // Update balance: current - withdraw_amount
        let current_balance: Felt = self.balances.get(&key);
        let new_balance = current_balance - withdraw_amount;
        self.balances.set(key, new_balance);

        // Create a P2ID note to send the requested asset back to the depositor
        self.create_p2id_note(serial_num, &withdraw_asset, depositor, tag, aux, note_type);
    }

    /// Create a P2ID (Pay-to-ID) note to send assets to a recipient.
    ///
    /// # Arguments
    /// * `serial_num` - Unique serial number for the note
    /// * `asset` - The asset to include in the note
    /// * `recipient_id` - The AccountId that can consume this note
    /// * `tag` - The note tag (passed by caller to allow proper P2ID routing)
    /// * `aux` - Auxiliary data for application-specific purposes
    /// * `note_type` - Note type as Felt: 1 = Public, 2 = Private
    fn create_p2id_note(
        &mut self,
        serial_num: Word,
        asset: &Asset,
        recipient_id: AccountId,
        tag: Felt,
        aux: Felt,
        note_type: Felt,
    ) {
        // Convert the passed tag Felt to a Tag
        // The caller is responsible for computing the proper P2ID tag
        // (typically LocalAny with account ID bits embedded)
        let tag = Tag::from(tag);

        // Convert note_type Felt to NoteType
        // 1 = Public (stored on-chain), 2 = Private (off-chain)
        let note_type = NoteType::from(note_type);

        // Execution hint: always (standard P2ID behavior per miden-base)
        // This is hardcoded to match miden-base's standard P2ID note implementation
        // which uses NoteExecutionHint::always() - represented as 0 in Felt form
        let execution_hint = felt!(0);

        // Get the P2ID note script root digest
        let script_root = Self::p2id_note_root();

        // Compute the recipient hash from:
        // - serial_num: unique identifier for this note instance
        // - script_root: the P2ID note script's MAST root
        // - inputs: the target account ID (padded to 8 elements)
        //
        // The P2ID script expects inputs as [suffix, prefix, 0, 0, 0, 0, 0, 0]
        let recipient = Recipient::compute(
            serial_num,
            script_root,
            vec![
                recipient_id.suffix,
                recipient_id.prefix,
                felt!(0),
                felt!(0),
                felt!(0),
                felt!(0),
                felt!(0),
                felt!(0),
            ],
        );

        // Create the output note
        let note_idx = output_note::create(tag, aux, note_type, execution_hint, recipient);

        // Remove the asset from the bank's vault
        native_account::remove_asset(asset.clone());

        // Add the asset to the output note
        output_note::add_asset(asset.clone(), note_idx);
    }
}
