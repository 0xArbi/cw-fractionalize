use crate::response::MsgInstantiateContractResponse;
#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    to_binary, Binary, ContractResult, Deps, DepsMut, Env, Event, MessageInfo, Order, Reply,
    ReplyOn, Response, StdError, StdResult, Uint128, Querier, from_binary,
};
use cw2::set_contract_version;
use protobuf::Message;

use crate::error::ContractError;
use crate::msg::{ExecuteMsg, GetCountResponse, GetCw20AddressResponse, InstantiateMsg, QueryMsg, ReceiveMsg};
use crate::state::{Config, CONFIG, STATE};
use cw20::{Cw20Coin, Logo, MinterResponse, Cw20Contract, Cw20ReceiveMsg, };
use cw20_base::contract::instantiate as instantiateCw20;
use cw20_base::msg::InstantiateMsg as Cw20InstantiateMsg;
use cw20_base::msg::ExecuteMsg as Cw20ExecuteMsg;
use cw721_base::entry::instantiate as cw721instantiate;
use cw721_base::helpers::Cw721Contract as Cw721ContractHelper;
use cw721_base::msg::{
    ExecuteMsg as Cw721ExecuteMsg, InstantiateMsg as Cw721InstantiateMsg, MintMsg,
};
use cw721_base::Cw721Contract;
use sg721::CollectionInfo;
use sg_std::StargazeMsgWrapper;
use std::marker::PhantomData;

use cosmwasm_std::{CosmosMsg, Empty, SubMsg, WasmMsg};

// version info for migration info
const CONTRACT_NAME: &str = "crates.io:cw-fractionalize";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

const INSTANTIATE_REPLY_ID: u64 = 1;

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
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
        ExecuteMsg::Fractionalize {
            address,
            token_id,
            owners,
        } => fractionalize(deps, info, env, address, token_id, owners),
        ExecuteMsg::Receive(msg) => handle_receive(deps, info, env, msg),
    }
}

pub fn fractionalize(
    deps: DepsMut,
    info: MessageInfo,
    env: Env,
    collection: String,
    token_id: String,
    initial_balances: Vec<Cw20Coin>,
) -> Result<Response, ContractError> {
    let collection_address = deps.api.addr_validate(&collection).unwrap();
    let nft =
        Cw721ContractHelper::<Empty, Empty>(collection_address.clone(), PhantomData, PhantomData);

    let owner = nft.owner_of(&deps.querier, token_id.clone(), false)?.owner;
    if owner != info.sender.to_string() {
        return Err(ContractError::Unauthorized {});
    }

    let _ = nft
        .approval(
            &deps.querier,
            token_id.clone(),
            env.contract.address.to_string(),
            None,
        )?
        .approval;

    let exists = STATE.has(deps.storage, (collection_address.clone(), token_id.clone()));
    if exists {
        return Err(ContractError::Exists {});
    }

    CONFIG.save(
        deps.storage,
        &Config {
            last_nft_fractionalized: (collection_address.clone(), token_id.clone()),
        },
    );

    Ok(Response::new()
        .add_submessage(SubMsg {
            id: INSTANTIATE_REPLY_ID,
            msg: CosmosMsg::Wasm(WasmMsg::Instantiate {
                admin: None,
                code_id: 1,
                msg: to_binary(&Cw20InstantiateMsg {
                    name: "name".to_string(),
                    symbol: "symbol".to_string(),
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
        })
        .add_submessage(SubMsg::new(WasmMsg::Execute {
            contract_addr: collection.to_string(),
            msg: to_binary(&Cw721ExecuteMsg::<Empty, Empty>::TransferNft {
                recipient: env.contract.address.clone().to_string(),
                token_id: token_id.to_string(),
            })?,
            funds: vec![],
        }))
    )

}

pub fn handle_receive(
    deps: DepsMut,
    info: MessageInfo,
    env: Env,
    wrapped: Cw20ReceiveMsg,
) -> Result<Response, ContractError> {
    let msg: ReceiveMsg = from_binary(&wrapped.msg)?;
    match msg {
        ReceiveMsg::Unfractionalize { address, token_id, recipient } => unfractionalize(deps, info, env, address, token_id, recipient, wrapped.amount),
    }
}

pub fn unfractionalize (
    deps: DepsMut, 
    info: MessageInfo, 
    env: Env, 
    collection: String, 
    token_id: String, 
    recipient: String,
    amount: Uint128,
) -> Result<Response, ContractError> {
    let collection_address = deps.api.addr_validate(&collection).unwrap();
    let nft =
        Cw721ContractHelper::<Empty, Empty>(collection_address.clone(), PhantomData, PhantomData);
    let owner = nft.owner_of(&deps.querier, token_id.clone(), false)?.owner;
    if owner != env.contract.address.to_string() {
        return Err(ContractError::Unauthorized {});
    }

    let cw20 = STATE.load(deps.storage, (collection_address.clone(), token_id.clone())).unwrap();
    let cw20_info: cw20::TokenInfoResponse = deps.querier.query_wasm_smart(
        cw20.clone(),
        &cw20_base::msg::QueryMsg::TokenInfo {
        },
    )?;
    if amount != cw20_info.total_supply {
        return Err(ContractError::Unauthorized {});
    }
    
    STATE.remove(deps.storage, (collection_address.clone(), token_id.clone()));

    Ok(Response::new()
        .add_submessage(SubMsg::new(WasmMsg::Execute {
            contract_addr: collection.to_string(),
            msg: to_binary(&Cw721ExecuteMsg::<Empty, Empty>::TransferNft {
                recipient,
                token_id: token_id.to_string(),
            })?,
            funds: vec![],
        }))
        .add_submessage(SubMsg::new(WasmMsg::Execute {
            contract_addr: cw20,
            msg: to_binary(&Cw20ExecuteMsg::Burn { 
                amount: cw20_info.total_supply
            })?,
            funds: vec![],
        }))
    )

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
    STATE.save(deps.storage, (collection_address, token_id), &cw20_address);

    Ok(Response::new())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::GetCount {} => to_binary(&count(deps)?),
        QueryMsg::GetCw20Address { address, token_id } => {
            to_binary(&getCw20Address(deps, address, token_id)?)
        }
    }
}

pub fn count(deps: Deps) -> StdResult<GetCountResponse> {
    let polls = STATE.range(deps.storage, None, None, Order::Ascending);
    let count = polls.count();
    Ok(GetCountResponse {
        count: count as i32,
    })
}

pub fn getCw20Address(
    deps: Deps,
    address: String,
    token_id: String,
) -> StdResult<GetCw20AddressResponse> {
    let nft_address = deps.api.addr_validate(&address).unwrap();
    let cw20_address = STATE.load(deps.storage, (nft_address, token_id)).unwrap();

    Ok(GetCw20AddressResponse {
        address: cw20_address,
    })
}


#[cfg(test)]
mod tests {
    use super::*;
    use cosmwasm_std::{
        testing::{mock_dependencies, mock_env, mock_info, MockApi, MockStorage, MockQuerier},
        Addr, Empty, Api, OwnedDeps, MemoryStorage,
    };
    use cw20::Cw20QueryMsg;
    use cw721::Cw721QueryMsg;
    use cw721::{NumTokensResponse, OwnerOfResponse};
    use cw721::{Cw721Query, Expiration};
    use cw721_base::{Cw721Contract, Extension};
    use cw_multi_test::{App, BankKeeper, Contract, ContractWrapper, Executor};


    pub fn nft_owner_of (router: &mut App, collection: String, token_id: String) -> String {
        let msg = Cw721QueryMsg::OwnerOf { token_id, include_expired: None };
        let res: OwnerOfResponse = router
            .wrap()
            .query_wasm_smart(collection, &msg)
            .unwrap();
        return res.owner;
    }

    pub fn nft_total (router: &mut App, collection: Addr) -> u64 {
        let msg = Cw721QueryMsg::NumTokens {};
        let res: NumTokensResponse = router
            .wrap()
            .query_wasm_smart(collection, &msg)
            .unwrap();
        return res.count;
    }

    pub fn token_balance (router: &mut App, token: String, user: String) -> Uint128 {
        let msg = Cw20QueryMsg::Balance {
            address: user.to_string(),
        };
        let res: cw20::BalanceResponse = router
            .wrap()
            .query_wasm_smart(token.to_string(), &msg)
            .unwrap();
        res.balance
    }

    pub fn token_transfer (router: &mut App, sender: Addr, token: Addr, amount: Uint128, recipient: Addr) {
        let msg = Cw20ExecuteMsg::Transfer { 
            recipient: recipient.to_string(), 
            amount,
        };
        let _ = router
            .execute_contract(sender, token, &msg, &[])
            .unwrap();
    }

    pub fn mint_nft (router: &mut App, minter: Addr, collection: Addr, token_id: String, user: Addr) {
        let mint_msg = Cw721ExecuteMsg::Mint::<_, Extension>(MintMsg::<Extension> {
            token_id: token_id.clone(),
            owner: user.to_string(),
            token_uri: Some("".to_string()),
            extension: None,
        });
        let _ = router
            .execute_contract(minter.clone(), collection, &mint_msg, &[])
            .unwrap();
    }

    pub fn approve_nft (router: &mut App, approver: Addr, spender: Addr, collection: Addr, token_id: String) {
        let msg = Cw721ExecuteMsg::<Empty, Empty>::Approve {
            spender: spender.to_string(),
            token_id: token_id,
            expires: None,
        };
        let _ = router
            .execute_contract(approver, collection, &msg, &[])
            .unwrap();
    }

    pub fn get_fractional_address (router: &mut App, fractionalizer_address: Addr, collection: Addr, token_id: String) -> String {
        let cw20_address: GetCw20AddressResponse = router
            .wrap()
            .query_wasm_smart(fractionalizer_address, &QueryMsg::GetCw20Address {
                address: collection.to_string(),
                token_id: token_id.to_string(),
            })
            .unwrap();
        return cw20_address.address
    }

    pub fn fractionalize (router: &mut App, sender: Addr, fractionalizer_address: Addr, collection: Addr, token_id: String, owners: Vec<Cw20Coin>) {
        let msg = ExecuteMsg::Fractionalize {
            address: collection.to_string(),
            token_id,
            owners,
        };
        router
            .execute_contract(sender, fractionalizer_address, &msg, &[])
            .unwrap();
    }

    pub fn contract_cw721() -> Box<dyn Contract<Empty>> {
        let contract = ContractWrapper::new(
            cw721_base::entry::execute,
            cw721_base::entry::instantiate,
            cw721_base::entry::query,
        );
        Box::new(contract)
    }

    pub fn contract_cw20() -> Box<dyn Contract<Empty>> {
        let contract = ContractWrapper::new(
            cw20_base::contract::execute,
            cw20_base::contract::instantiate,
            cw20_base::contract::query,
        );
        Box::new(contract)
    }

    pub fn contract_fractionalizer() -> Box<dyn Contract<Empty>> {
        let contract = ContractWrapper::new(
            execute, 
            instantiate, 
            query
        ).with_reply(reply);
        Box::new(contract)
    }

    struct World {
        deps: OwnedDeps<MemoryStorage, MockApi, MockQuerier>,

        deployer_address: Addr,
        user_one: Addr,
        user_two: Addr,
        nft_address: Addr,
        fractionalizer_address: Addr,
    }

    fn setup (router: &mut App) -> World {
        let deployer = mock_info("deployer", &[]);

        let mut deps = mock_dependencies();
        let env = mock_env();

        let user_one = mock_info("user_one", &[]);
        let user_two = mock_info("user_two", &[]);

        // CW20
        router.store_code(contract_cw20());

        // NFT
        let contract_code_id = router.store_code(contract_cw721());
        let msg = Cw721InstantiateMsg {
            minter: deployer.sender.clone().into_string(),
            name: "Mock NFT".to_string(),
            symbol: "MOCK".to_string(),
        };
        let nft_address = router
            .instantiate_contract(contract_code_id, deployer.clone().sender, &msg, &[], "counter", None)
            .unwrap();

        // Fractionalizer
        let contract_code_id = router.store_code(contract_fractionalizer());
        let msg = InstantiateMsg {};
        let fractionalizer_address = router
            .instantiate_contract(contract_code_id, deployer.clone().sender, &msg, &[], "counter", None)
            .unwrap();

        let nft_address = deps.api.addr_validate(&nft_address.to_string()).unwrap();

        return World {
            deps: deps,

            deployer_address: deployer.sender,
            fractionalizer_address: fractionalizer_address,
            nft_address,
            user_one: user_one.sender,
            user_two: user_two.sender,
        }

    }

     fn mock_app() -> App {
        let env = mock_env();
        let api = Box::new(MockApi::default());
        let bank = BankKeeper::new();

        // App::new(api, env.block, bank, Box::new(MockStorage::new()))
        App::new(|a, b, c| {})
    }

     #[test]
    fn test_setup() {
        let router = &mut mock_app();
        let world = setup(router);

        let msg = Cw721QueryMsg::NumTokens {};
        let res: NumTokensResponse = router
            .wrap()
            .query_wasm_smart(world.nft_address, &msg)
            .unwrap();

        assert_eq!(0, res.count);

        let msg = QueryMsg::GetCount {};
        let res: GetCountResponse = router
            .wrap()
            .query_wasm_smart(world.fractionalizer_address, &msg)
            .unwrap();
        assert_eq!(res.count, 0);
    }

    #[test]
    fn test_mint() {
        let router = &mut mock_app();
        let w = setup(router);

        let token_id = "nft".to_string();
        mint_nft(router, w.deployer_address.clone(), w.nft_address.clone(), token_id.clone(), w.deployer_address.clone());

        let owner_of = nft_owner_of(router, w.nft_address.clone().to_string(), token_id.to_string());
        assert_eq!(owner_of, w.deployer_address.clone().to_string());
    }

    #[test]
    fn test_fractionalize() {
        let router = &mut mock_app();
        let w = setup(router);

        // mint & approve NFT
        let token_id = "nft".to_string();
        mint_nft(router, w.deployer_address.clone(), w.nft_address.clone(), token_id.clone(), w.deployer_address.clone());

        approve_nft(router, w.deployer_address.clone(), w.fractionalizer_address.clone(), w.nft_address.clone(), token_id.clone());

        fractionalize(router, w.deployer_address.clone(), w.fractionalizer_address.clone(), w.nft_address.clone(), token_id.clone(),  vec![
            Cw20Coin {
                address: w.user_one.clone().to_string(),
                amount: Uint128::from(1u128),
            },
            Cw20Coin {
                address: w.user_two.clone().to_string(),
                amount: Uint128::from(2u128),
            },
        ]);

        let cw20 = get_fractional_address(router, w.fractionalizer_address.clone(), w.nft_address.clone(), token_id.clone());

        // assertions
        let owner_of = nft_owner_of(router, w.nft_address.clone().to_string(), token_id.to_string());
        assert_eq!(owner_of, w.fractionalizer_address.clone().to_string());

        let bal = token_balance(router, cw20.clone(), w.user_one.clone().to_string());
        assert_eq!(bal, Uint128::from(1u128));

        let bal = token_balance(router, cw20, w.user_two.to_string());
        assert_eq!(bal, Uint128::from(2u128));
    }

    #[test] 
    fn test_unfractionalize() {
        let router = &mut mock_app();
        let w = setup(router);

        // mint & approve NFT
        let token_id = "nft".to_string();
        mint_nft(router, w.deployer_address.clone(), w.nft_address.clone(), token_id.clone(), w.deployer_address.clone());

        approve_nft(router, w.deployer_address.clone(), w.fractionalizer_address.clone(), w.nft_address.clone(), token_id.clone());

        fractionalize(router, w.deployer_address.clone(), w.fractionalizer_address.clone(), w.nft_address.clone(), token_id.clone(),  vec![
            Cw20Coin {
                address: w.user_one.clone().to_string(),
                amount: Uint128::from(1u128),
            },
            Cw20Coin {
                address: w.user_two.clone().to_string(),
                amount: Uint128::from(2u128),
            },
        ]);

        let cw20 = get_fractional_address(router, w.fractionalizer_address.clone(), w.nft_address.clone(), token_id.clone());
        let cw20_address = w.deps.api.addr_validate(&cw20).unwrap();
        token_transfer(router, w.user_two.clone(), cw20_address.clone(), Uint128::from(2u128), w.user_one.clone());

        let bal = token_balance(router, cw20.clone(), w.user_one.clone().to_string());
        assert_eq!(bal, Uint128::from(3u128));

        let msg = Cw20ExecuteMsg::Send { 
            contract: w.fractionalizer_address.clone().to_string(), 
            amount: bal, 
            msg: to_binary(&ReceiveMsg::Unfractionalize { 
                address: w.nft_address.clone().to_string(), 
                token_id: token_id.clone(),
                recipient: w.user_one.clone().to_string(),
            }).unwrap(),
        };
        let _ = router
            .execute_contract(w.user_one.clone(), cw20_address.clone(), &msg, &[])
            .unwrap();
        let msg = Cw20ExecuteMsg::Send { 
            contract: w.fractionalizer_address.clone().to_string(), 
            amount: Uint128::from(1u128), 
            msg: to_binary(&ReceiveMsg::Unfractionalize { 
                address: w.nft_address.clone().to_string(), 
                token_id: token_id.clone(),
                recipient: w.user_one.clone().to_string(),
            }).unwrap(),
        };
        let _ = router
            .execute_contract(w.user_one.clone(), cw20_address.clone(), &msg, &[])
            .unwrap_err();

        // assertions
        let owner_of = nft_owner_of(router, w.nft_address.clone().to_string(), token_id.to_string());
        assert_eq!(owner_of, w.user_one.clone().to_string());

        let bal = token_balance(router, cw20.clone(), w.user_one.clone().to_string());
        assert_eq!(bal, Uint128::from(0u128));

        let bal = token_balance(router, cw20.clone(), w.user_two.to_string());
        assert_eq!(bal, Uint128::from(0u128));
        
        let bal = token_balance(router, cw20, w.fractionalizer_address.to_string());
        assert_eq!(bal, Uint128::from(0u128));
    }
}
