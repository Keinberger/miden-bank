# Miden Bank

A complete banking application built with the Miden Rust compiler, demonstrating deposits, withdrawals, and asset management on the Miden protocol.

This repository serves as the companion code for the **Building a Bank with Miden Rust** tutorial in the [Miden Documentation](https://docs.miden.xyz).

## Overview

This banking system showcases all major Miden Rust compiler concepts:

- **Account Components** - Smart contracts with persistent storage
- **Note Scripts** - Code that executes when notes are consumed
- **Transaction Scripts** - Owner-initiated account operations
- **Cross-Component Calls** - Communication between contracts
- **Output Notes** - Programmatic note creation (P2ID pattern)

## Repository Structure

```
miden-bank/
├── contracts/
│   ├── bank-account/           # Main bank account component
│   │   ├── Cargo.toml
│   │   └── src/lib.rs
│   ├── deposit-note/           # Note script for deposits
│   │   ├── Cargo.toml
│   │   └── src/lib.rs
│   ├── withdraw-request-note/  # Note script for withdrawal requests
│   │   ├── Cargo.toml
│   │   └── src/lib.rs
│   └── init-tx-script/         # Transaction script for initialization
│       ├── Cargo.toml
│       └── src/lib.rs
├── integration/
│   ├── src/
│   │   └── helpers.rs          # Test utilities
│   └── tests/
│       ├── deposit_test.rs     # Deposit flow tests
│       └── withdraw_test.rs    # Withdrawal flow tests
└── Cargo.toml                  # Workspace configuration
```

## Components

### Bank Account (`contracts/bank-account`)

The core account component that:
- Tracks depositor balances in a `StorageMap`
- Manages an initialization flag in `Value` storage
- Enforces a maximum deposit limit (1,000,000 tokens)
- Creates P2ID output notes for withdrawals

### Deposit Note (`contracts/deposit-note`)

A note script that:
- Retrieves the sender (depositor) via `active_note::get_sender()`
- Gets attached assets via `active_note::get_assets()`
- Calls `bank_account::deposit()` to credit the depositor

### Withdraw Request Note (`contracts/withdraw-request-note`)

A note script that:
- Parses withdrawal parameters from note inputs
- Calls `bank_account::withdraw()` to process the request
- Triggers P2ID note creation for asset transfer

### Init Transaction Script (`contracts/init-tx-script`)

A transaction script that:
- Initializes the bank account
- Enables deposits by setting the initialized flag
- Makes the account visible on-chain

## Prerequisites

- [Rust](https://rustup.rs/) (latest stable)
- [Miden CLI](https://docs.miden.xyz) (`midenup`)

Install the Miden toolchain:

```bash
curl -sSL https://raw.githubusercontent.com/0xMiden/midenup/main/install.sh | bash
midenup
```

## Building

Build all contracts in the correct dependency order:

```bash
# Build the bank account component first
cd contracts/bank-account
miden build

# Build note scripts (depend on bank-account)
cd ../deposit-note
miden build

cd ../withdraw-request-note
miden build

# Build transaction script
cd ../init-tx-script
miden build
```

## Testing

Run the integration tests:

```bash
# Run all tests
cargo test -p integration -- --nocapture

# Run specific tests
cargo test -p integration deposit_test -- --nocapture
cargo test -p integration withdraw_test -- --nocapture

# Test failure cases
cargo test -p integration deposit_exceeds_max_should_fail -- --nocapture
cargo test -p integration deposit_without_init_should_fail -- --nocapture
```

## Tutorial

This repository accompanies the multi-part tutorial covering:

1. **Account Components and Storage** - `#[component]`, `Value`, `StorageMap`
2. **Constants and Constraints** - Business rules with `assert!()`
3. **Asset Management** - `native_account::add_asset()` and `remove_asset()`
4. **Note Scripts** - `#[note]` struct + impl pattern, `active_note::` APIs
5. **Cross-Component Calls** - Generated bindings and dependencies
6. **Transaction Scripts** - `#[tx_script]`, initialization patterns
7. **Creating Output Notes** - P2ID pattern, `Recipient::compute()`
8. **Complete Flows** - End-to-end deposit and withdrawal

## Key Concepts Demonstrated

### Storage Types

```rust
#[component]
struct Bank {
    #[storage(description = "initialized")]
    initialized: Value,

    #[storage(description = "balances")]
    balances: StorageMap,
}
```

### Note Script Pattern

```rust
#[note]
struct DepositNote;

#[note]
impl DepositNote {
    #[note_script]
    fn run(self, _arg: Word) {
        let depositor = active_note::get_sender();
        let assets = active_note::get_assets();

        for asset in assets {
            bank_account::deposit(depositor, asset);
        }
    }
}
```

### Transaction Script Pattern

```rust
#[tx_script]
fn run(_arg: Word, account: &mut Account) {
    account.initialize();
}
```

### P2ID Note Creation

```rust
let recipient = Recipient::compute(serial_num, script_root, inputs);
let note_idx = output_note::create(tag, note_type, recipient);
native_account::remove_asset(asset.clone());
output_note::add_asset(asset.clone(), note_idx);
```

## License

This project is licensed under the MIT License.

## Resources

- [Miden Documentation](https://docs.miden.xyz)
- [Miden Rust Compiler](https://github.com/0xMiden/compiler)
- [Miden VM](https://github.com/0xMiden/miden-vm)
- [Build On Miden Telegram](https://t.me/BuildOnMiden)
