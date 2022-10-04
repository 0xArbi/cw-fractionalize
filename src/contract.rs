#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    from_binary, to_binary, Addr, Binary, CosmosMsg, Deps, DepsMut, Empty, Env, MessageInfo, Reply,
    ReplyOn, Response, StdError, StdResult, SubMsg, Uint128, WasmMsg,
};

use cw2::set_contract_version;
use cw20::{Cw20Coin, Cw20ReceiveMsg};
use cw721::Cw721ReceiveMsg;
use cw20_base::msg::{ExecuteMsg as Cw20ExecuteMsg, InstantiateMsg as Cw20InstantiateMsg};
use cw721_base::msg::ExecuteMsg as Cw721ExecuteMsg;
use protobuf::Message;

use crate::response::MsgInstantiateContractResponse;
use crate::error::ContractError;
use crate::msg::{ExecuteMsg, GetCw20AddressResponse, InstantiateMsg, QueryMsg, ReceiveMsg};
use crate::state::{Config, CONFIG, CW20_NFT, NFT_CW20};

const CONTRACT_NAME: &str = "crates.io:cw-fractionalize";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

const INSTANTIATE_REPLY_ID: u64 = 1;

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    _msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    Ok(Response::new())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::ReceiveNft(msg) => handle_fractionalize(deps, info, msg),
        ExecuteMsg::Receive(msg) => handle_unfractionalize(deps, info, env, msg),
    }
}

pub fn fractionalize(
    deps: DepsMut,
    collection: Addr,
    token_id: String,
    initial_balances: Vec<Cw20Coin>,
    name: String,
    symbol: String
) -> Result<Response, ContractError> {
    let exists = NFT_CW20.has(deps.storage, (collection.clone(), token_id.clone()));
    if exists {
        return Err(ContractError::Exists {});
    }

    CONFIG.save(
        deps.storage,
        &Config {
            last_nft_fractionalized: (collection, token_id),
        },
    );

    Ok(Response::new().add_submessage(SubMsg {
        id: INSTANTIATE_REPLY_ID,
        msg: CosmosMsg::Wasm(WasmMsg::Instantiate {
            admin: None,
            code_id: 1,
            msg: to_binary(&Cw20InstantiateMsg {
                name: name.to_string(),
                symbol: symbol.to_string(),
                decimals: 6,
                initial_balances,
                mint: None,
                marketing: None,
            })?,
            funds: vec![],
            label: "label".to_string(),
        }),
        gas_limit: None,
        reply_on: ReplyOn::Success,
    }))
}

pub fn handle_unfractionalize(
    deps: DepsMut,
    info: MessageInfo,
    _env: Env,
    wrapped: Cw20ReceiveMsg,
) -> Result<Response, ContractError> {
    let msg: ReceiveMsg = from_binary(&wrapped.msg)?;
    match msg {
        ReceiveMsg::Unfractionalize { recipient } => {
            unfractionalize(deps, info.sender, recipient, wrapped.amount)
        }
        _ => Err(ContractError::Unauthorized {}),
    }
}

pub fn handle_fractionalize(
    deps: DepsMut,
    info: MessageInfo,
    wrapped: Cw721ReceiveMsg,
) -> Result<Response, ContractError> {
    let msg: ReceiveMsg = from_binary(&wrapped.msg)?;
    match msg {
        ReceiveMsg::Fractionalize { owners, name, symbol } => {
            fractionalize(deps, info.sender, wrapped.token_id, owners, name, symbol)
        }
        _ => Err(ContractError::Unauthorized {}),
    }
}

pub fn unfractionalize(
    deps: DepsMut,
    cw20_address: Addr,
    recipient: String,
    amount: Uint128,
) -> Result<Response, ContractError> {
    let data = CW20_NFT.may_load(deps.storage, cw20_address.to_string())?;
    if data.is_none() {
        return Err(ContractError::NotFractionalized {});
    }

    let (nft_address, token_id) = data.unwrap();

    let cw20_info: cw20::TokenInfoResponse = deps.querier.query_wasm_smart(
        cw20_address.clone(),
        &cw20_base::msg::QueryMsg::TokenInfo {},
    )?;
    if amount != cw20_info.total_supply {
        return Err(ContractError::InsufficientFunds {});
    }

    NFT_CW20.remove(deps.storage, (nft_address.clone(), token_id.clone()));
    CW20_NFT.remove(deps.storage, cw20_address.to_string());

    Ok(Response::new()
        .add_submessage(SubMsg::new(WasmMsg::Execute {
            contract_addr: nft_address.to_string(),
            msg: to_binary(&Cw721ExecuteMsg::<Empty, Empty>::TransferNft {
                recipient,
                token_id,
            })?,
            funds: vec![],
        }))
        .add_submessage(SubMsg::new(WasmMsg::Execute {
            contract_addr: cw20_address.to_string(),
            msg: to_binary(&Cw20ExecuteMsg::Burn {
                amount: cw20_info.total_supply,
            })?,
            funds: vec![],
        })))
}

// Reply callback triggered from cw721 contract instantiation
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn reply(deps: DepsMut, _env: Env, msg: Reply) -> Result<Response, ContractError> {
    let data = msg.result.unwrap().data.unwrap();
    let res: MsgInstantiateContractResponse =
        Message::parse_from_bytes(data.as_slice()).map_err(|_| {
            StdError::parse_err("MsgInstantiateContractResponse", "failed to parse data")
        })?;
    let cw20_address = res.get_address().to_string();

    let config = CONFIG.load(deps.storage).unwrap();
    let (collection_address, token_id) = config.last_nft_fractionalized;

    NFT_CW20.save(
        deps.storage,
        (collection_address.clone(), token_id.clone()),
        &cw20_address,
    );
    CW20_NFT.save(deps.storage, cw20_address, &(collection_address, token_id));

    Ok(Response::new())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::GetCw20Address { address, token_id } => {
            to_binary(&get_cw20_address(deps, address, token_id)?)
        }
    }
}

pub fn get_cw20_address(
    deps: Deps,
    address: String,
    token_id: String,
) -> StdResult<GetCw20AddressResponse> {
    let nft_address = deps.api.addr_validate(&address).unwrap();
    let cw20_address = NFT_CW20
        .load(deps.storage, (nft_address, token_id))
        .unwrap();

    Ok(GetCw20AddressResponse {
        address: cw20_address,
    })
}
