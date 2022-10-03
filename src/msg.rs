use cosmwasm_schema::{cw_serde, QueryResponses};
use cw20::Cw20Coin;

#[cw_serde]
pub struct InstantiateMsg {}

#[cw_serde]
pub enum ExecuteMsg {
    Fractionalise {
        address: String,
        token_id: String,
        owners: Vec<Cw20Coin>,
    },
}

#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {
    // GetCount returns the current count as a json-encoded number
    #[returns(GetCountResponse)]
    GetCount {},
    #[returns(GetCw20AddressResponse)]
    GetCw20Address { address: String, token_id: String },
}

// We define a custom struct for each query response
#[cw_serde]
pub struct GetCountResponse {
    pub count: i32,
}

#[cw_serde]
pub struct GetCw20AddressResponse {
    pub address: String,
}
