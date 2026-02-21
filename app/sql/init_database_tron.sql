CREATE DATABASE IF NOT EXISTS tron_db;

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
) ENGINE = ReplacingMergeTree(inserted_at)
ORDER BY address;

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
) ENGINE = ReplacingMergeTree(inserted_at)
ORDER BY (block_number, hash);

---------------------------------------------------------
-- OWNER INFO
---------------------------------------------------------
CREATE TABLE IF NOT EXISTS tron_db.owner_info (
    address String,
    person_name String,
    person_id String,
    personal_id UInt16,
    inserted_at DateTime DEFAULT now()
) ENGINE = ReplacingMergeTree(inserted_at)
ORDER BY address;

---------------------------------------------------------
-- ADDRESS TAGS
---------------------------------------------------------
CREATE TABLE IF NOT EXISTS tron_db.address_tags (
    address String,
    tag String,
    created_at DateTime DEFAULT now()
) ENGINE = MergeTree()
ORDER BY (address, tag);

---------------------------------------------------------
-- TOKEN TRANSFERS (CANONICAL TABLE)
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
) ENGINE = ReplacingMergeTree(inserted_at)
ORDER BY (tx_hash, log_index);

---------------------------------------------------------
-- TOKEN DELTA (CANONICAL TABLE)
---------------------------------------------------------
CREATE TABLE IF NOT EXISTS tron_db.address_token_delta (
    tx_hash String,
    log_index UInt32,
    direction UInt8,     -- 0 = outgoing, 1 = incoming
    address String,
    token_address String,
    delta Int256,
    block_number UInt64,
    inserted_at DateTime DEFAULT now()
) ENGINE = ReplacingMergeTree(inserted_at)
ORDER BY (tx_hash, log_index, direction);

---------------------------------------------------------
-- MV: DELTA FROM (SENDER)
---------------------------------------------------------
CREATE MATERIALIZED VIEW IF NOT EXISTS tron_db.mv_token_delta_from
TO tron_db.address_token_delta
AS
SELECT
    tx_hash,
    log_index,
    0 AS direction,
    from_addr AS address,
    token_address,
    -toInt256(amount) AS delta,
    block_number
FROM tron_db.token_transfers
WHERE from_addr != '0x0000000000000000000000000000000000000000';

---------------------------------------------------------
-- MV: DELTA TO (RECEIVER)
---------------------------------------------------------
CREATE MATERIALIZED VIEW IF NOT EXISTS tron_db.mv_token_delta_to
TO tron_db.address_token_delta
AS
SELECT
    tx_hash,
    log_index,
    1 AS direction,
    to_addr AS address,
    token_address,
    toInt256(amount) AS delta,
    block_number
FROM tron_db.token_transfers
WHERE to_addr != '0x0000000000000000000000000000000000000000';

---------------------------------------------------------
-- FINAL TOKEN BALANCE TABLE
---------------------------------------------------------
CREATE TABLE IF NOT EXISTS tron_db.address_token_balance (
    address String,
    token_address String,
    balance Int256
) ENGINE = SummingMergeTree()
ORDER BY (address, token_address);

---------------------------------------------------------
-- MV: FINAL BALANCE
---------------------------------------------------------
CREATE MATERIALIZED VIEW IF NOT EXISTS tron_db.mv_token_balance
TO tron_db.address_token_balance
AS
SELECT
    address,
    token_address,
    delta AS balance
FROM tron_db.address_token_delta;

---------------------------------------------------------
-- TOKEN META DETAILS
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
) ENGINE = ReplacingMergeTree(updated_at)
ORDER BY token_address;

---------------------------------------------------------
-- SYNC STATE
---------------------------------------------------------
CREATE TABLE IF NOT EXISTS tron_db.sync_state (
    chain String,
    last_synced_block UInt64,
    updated_at DateTime DEFAULT now()
) ENGINE = ReplacingMergeTree(updated_at)
ORDER BY chain;
