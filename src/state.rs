use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cosmwasm_std::Addr;
use cw_storage_plus::{Item, Map};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Config {
    pub last_nft_fractionalized: (Addr, String),
}

pub const STATE: Map<(Addr, String), String> = Map::new("nfts");
pub const CONFIG: Item<Config> = Item::new("config");
