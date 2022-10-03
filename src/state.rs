use std::vec;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cosmwasm_std::Addr;
use cw_storage_plus::{Item, Map, UniqueIndex, IndexList, Index, IndexedMap, MultiIndex};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Config {
    pub last_nft_fractionalized: (Addr, String),
}

pub const CW20_NFT: Map<String, (Addr, String)> = Map::new("CW20_NFT");
pub const NFT_CW20: Map<(Addr, String), String> = Map::new("NFT_CW20");
pub const CONFIG: Item<Config> = Item::new("config");
