CREATE DATABASE IF NOT EXISTS tron_db;

---------------------------------------------------------
-- SYNC STATE 
---------------------------------------------------------
CREATE TABLE IF NOT EXISTS tron_db.sync_state (
    chain String,
    last_synced_block UInt64,
    updated_at DateTime DEFAULT now()
)
ENGINE = ReplacingMergeTree(updated_at)
ORDER BY chain;

---------------------------------------------------------
-- TRANSACTIONS
---------------------------------------------------------
CREATE TABLE IF NOT EXISTS tron_db.transactions (
    hash String,
    block_number UInt64,
    from_addr String,
    to_addr String,
    value String,
    sensivity UInt8,
    inserted_at DateTime DEFAULT now()
)
ENGINE = ReplacingMergeTree(inserted_at)
ORDER BY (block_number, hash);

---------------------------------------------------------
-- TOKEN TRANSFERS
---------------------------------------------------------
CREATE TABLE IF NOT EXISTS tron_db.token_transfers (
    tx_hash String,
    block_number UInt64,
    log_index UInt32,
    token_address String,
    from_addr String,
    to_addr String,
    amount String,
    inserted_at DateTime DEFAULT now()
)
ENGINE = ReplacingMergeTree(inserted_at)
ORDER BY (tx_hash, log_index);

---------------------------------------------------------
-- WALLET INFO 
---------------------------------------------------------
CREATE TABLE IF NOT EXISTS tron_db.wallet_info (
    address String,
    balance String,
    nonce UInt64,
    type String,
    person_id String,
    inserted_at DateTime DEFAULT now()
)
ENGINE = ReplacingMergeTree(inserted_at)
ORDER BY address;

---------------------------------------------------------
-- OWNER INFO 
---------------------------------------------------------
CREATE TABLE IF NOT EXISTS tron_db.owner_info (
    address String,
    person_name String,
    person_id String,
    personal_id UInt16,
    inserted_at DateTime DEFAULT now()
)
ENGINE = ReplacingMergeTree(inserted_at)
ORDER BY address;

---------------------------------------------------------
-- ADDRESS TAGS 
---------------------------------------------------------
CREATE TABLE IF NOT EXISTS tron_db.address_tags (
    address String,
    tag String,
    created_at DateTime DEFAULT now()
)
ENGINE = MergeTree
ORDER BY (address, tag);

---------------------------------------------------------
-- TOKEN METADATA 
---------------------------------------------------------
CREATE TABLE IF NOT EXISTS tron_db.token_metadata (
    token_address String,
    name String,
    symbol String,
    decimals UInt8,
    total_supply String,
    is_verified UInt8,
    created_at DateTime DEFAULT now(),
    updated_at DateTime DEFAULT now()
)
ENGINE = ReplacingMergeTree(updated_at)
ORDER BY token_address;

---------------------------------------------------------
-- AML TABLES (ADD ONLY)
---------------------------------------------------------
CREATE TABLE IF NOT EXISTS tron_db.contract_calls (
    tx_hash String,
    block_number UInt64,
    owner_address String,
    contract_address String,
    call_value String,
    selector FixedString(8),
    data String,
    success UInt8,
    inserted_at DateTime DEFAULT now()
)
ENGINE = ReplacingMergeTree(inserted_at)
ORDER BY (block_number, tx_hash);

CREATE TABLE IF NOT EXISTS tron_db.token_balance_snapshot (
    tx_hash String,
    block_number UInt64,
    address String,
    token_address String,
    balance_before String,
    balance_after String,
    inserted_at DateTime DEFAULT now()
)
ENGINE = ReplacingMergeTree(inserted_at)
ORDER BY (block_number, tx_hash, address);

CREATE TABLE IF NOT EXISTS tron_db.money_flows (
    tx_hash String,
    block_number UInt64,
    from_address String,
    to_address String,
    token_address String,
    amount String,
    action Enum8(
        'TRANSFER' = 1,
        'SWAP' = 2,
        'BRIDGE' = 3,
        'CEX' = 4,
        'MIXER' = 5,
        'UNKNOWN' = 99
    ),
    inserted_at DateTime DEFAULT now()
)
ENGINE = ReplacingMergeTree(inserted_at)
ORDER BY (block_number, tx_hash);