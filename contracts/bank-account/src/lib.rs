// Do not link against libstd (i.e. anything defined in `std::`)
#![no_std]
#![feature(alloc_error_handler)]

#[macro_use]
extern crate alloc;

use miden::*;


/// Bank account component that tracks depositor balances.
///
/// Users deposit assets via deposit notes, and the bank tracks
/// each depositor's balance in a storage map keyed by their AccountId.
#[component]
struct Bank {
    /// Maps depositor AccountId -> balance (as Felt)
    /// Key is derived from AccountId: [prefix, suffix, asset_prefix, asset_suffix]
    #[storage(slot(0), description = "balances")]
    balances: StorageMap,
}

#[component]
impl Bank {
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
    pub fn deposit(&mut self, depositor: AccountId, deposit_asset: Asset) {
        // Extract the fungible amount from the asset
        // Asset inner layout for fungible: [amount, 0, faucet_suffix, faucet_prefix]
        // Asset.inner is a Word field, access it directly
        let deposit_amount = deposit_asset.inner[0];

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
    pub fn withdraw(&mut self, depositor: AccountId, withdraw_asset: Asset, serial_num: Word, tag: Felt) {
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
        self.create_p2id_note(serial_num, &withdraw_asset, depositor, tag);
    }

    /// Create a P2ID (Pay-to-ID) note to send assets to a recipient.
    ///
    /// # Arguments
    /// * `serial_num` - Unique serial number for the note
    /// * `asset` - The asset to include in the note
    /// * `recipient_id` - The AccountId that can consume this note
    /// * `tag` - The note tag (passed by caller to allow proper P2ID routing)
    fn create_p2id_note(&mut self, serial_num: Word, asset: &Asset, recipient_id: AccountId, tag: Felt) {
        // Convert the passed tag Felt to a Tag
        // The caller is responsible for computing the proper P2ID tag
        // (typically LocalAny with account ID bits embedded)
        let tag = Tag::from(tag);

        // Auxiliary data - can be used for application-specific purposes
        let aux = felt!(0);

        // Note type: Public (1)
        // Public notes have their full data stored on-chain
        let note_type = NoteType::from(Felt::from_u32(1));

        // Execution hint: None (0)
        // No specific execution timing requirements
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
