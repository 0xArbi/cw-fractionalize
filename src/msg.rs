use cosmwasm_schema::{cw_serde, QueryResponses};
use cw20::{Cw20Coin, Cw20ReceiveMsg};
use cw721::Cw721ReceiveMsg;

#[cw_serde]
pub struct InstantiateMsg {}

#[cw_serde]
pub enum ExecuteMsg {
    Receive(Cw20ReceiveMsg),
    ReceiveNft(Cw721ReceiveMsg),
}

#[cw_serde]
pub enum ReceiveMsg {
    Fractionalize { owners: Vec<Cw20Coin>, name: String, symbol: String },
    Unfractionalize { recipient: String },
}

#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {
    #[returns(GetCw20AddressResponse)]
    GetCw20Address { address: String, token_id: String },
}

#[cw_serde]
pub struct GetCw20AddressResponse {
    pub address: String,
}
