use crate::response::MsgInstantiateContractResponse;
#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    to_binary, Binary, ContractResult, Deps, DepsMut, Env, Event, MessageInfo, Order, Reply,
    ReplyOn, Response, StdError, StdResult, Uint128,
};
use cw2::set_contract_version;
use protobuf::Message;

use crate::error::ContractError;
use crate::msg::{ExecuteMsg, GetCountResponse, GetCw20AddressResponse, InstantiateMsg, QueryMsg};
use crate::state::{Config, CONFIG, STATE};
use cw20::{Cw20Coin, Logo, MinterResponse};
use cw20_base::contract::instantiate as instantiateCw20;
use cw20_base::msg::InstantiateMsg as Cw20InstantiateMsg;
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
        ExecuteMsg::Fractionalise {
            address,
            token_id,
            owners,
        } => fractionalize(deps, info, env, address, token_id, owners),
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
            last_nft_fractionalized: (collection_address, token_id),
        },
    );

    Ok(Response::new().add_submessage(SubMsg {
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
    }))
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
        testing::{mock_dependencies, mock_env, mock_info, MockApi, MockStorage},
        Addr, Empty,
    };
    use cw20::Cw20QueryMsg;
    use cw721::Cw721QueryMsg;
    use cw721::NumTokensResponse;
    use cw721::{Cw721Query, Expiration};
    use cw721_base::{Cw721Contract, Extension};
    use cw_multi_test::{App, BankKeeper, Contract, ContractWrapper, Executor};

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
        let contract = ContractWrapper::new(execute, instantiate, query).with_reply(reply);
        Box::new(contract)
    }

    #[test]
    fn test_s() {
        let deployer = mock_info("deployer", &[]);

        let mut deps = mock_dependencies();
        let env = mock_env();
        let owner = Addr::unchecked("owner");

        fn mock_app() -> App {
            let env = mock_env();
            let api = Box::new(MockApi::default());
            let bank = BankKeeper::new();

            // App::new(api, env.block, bank, Box::new(MockStorage::new()))
            App::new(|a, b, c| {})
        }

        let user_one = mock_info("user_one", &[]);
        let user_two = mock_info("user_two", &[]);

        let mut router = mock_app();

        let cw20_contract_code_id = router.store_code(contract_cw20());

        // deploy NFT
        let contract_code_id = router.store_code(contract_cw721());
        let msg = Cw721InstantiateMsg {
            minter: deployer.sender.clone().into_string(),
            name: "Mock NFT".to_string(),
            symbol: "MOCK".to_string(),
        };
        let nft_address = router
            .instantiate_contract(contract_code_id, owner.clone(), &msg, &[], "counter", None)
            .unwrap();

        // deploy Fractionalizer
        let contract_code_id = router.store_code(contract_fractionalizer());
        let msg = InstantiateMsg {};
        let fractionalizer_address = router
            .instantiate_contract(contract_code_id, owner.clone(), &msg, &[], "counter", None)
            .unwrap();

        let msg = Cw721QueryMsg::NumTokens {};
        let res: NumTokensResponse = router
            .wrap()
            .query_wasm_smart(nft_address.clone(), &msg)
            .unwrap();

        assert_eq!(0, res.count);

        let token_id = "petrify".to_string();
        let token_uri = "https://www.merriam-webster.com/dictionary/petrify".to_string();

        let mint_msg = Cw721ExecuteMsg::Mint::<_, Extension>(MintMsg::<Extension> {
            token_id: token_id.clone(),
            owner: deployer.clone().sender.to_string(),
            token_uri: Some(token_uri.clone()),
            extension: None,
        });
        let _ = router
            .execute_contract(deployer.sender.clone(), nft_address.clone(), &mint_msg, &[])
            .unwrap();

        let msg = Cw721QueryMsg::NumTokens {};
        let res: NumTokensResponse = router
            .wrap()
            .query_wasm_smart(nft_address.clone(), &msg)
            .unwrap();
        assert_eq!(1, res.count);

        let msg = Cw721ExecuteMsg::<Empty, Empty>::Approve {
            spender: fractionalizer_address.clone().to_string(),
            token_id: token_id.clone(),
            expires: None,
        };
        let _ = router
            .execute_contract(deployer.sender.clone(), nft_address.clone(), &msg, &[])
            .unwrap();

        // start
        let msg = QueryMsg::GetCount {};
        let res: GetCountResponse = router
            .wrap()
            .query_wasm_smart(fractionalizer_address.clone(), &msg)
            .unwrap();
        assert_eq!(res.count, 0);

        let owners = vec![
            Cw20Coin {
                address: user_one.sender.clone().to_string(),
                amount: Uint128::from(1u128),
            },
            Cw20Coin {
                address: user_two.sender.clone().to_string(),
                amount: Uint128::from(2u128),
            },
        ];
        let msg = ExecuteMsg::Fractionalise {
            address: nft_address.clone().to_string(),
            token_id: token_id.clone(),
            owners,
        };
        router
            .execute_contract(
                deployer.sender.clone(),
                fractionalizer_address.clone(),
                &msg,
                &[],
            )
            .unwrap();

        let msg = QueryMsg::GetCw20Address {
            address: nft_address.clone().to_string(),
            token_id: token_id.clone().to_string(),
        };
        let address_res: GetCw20AddressResponse = router
            .wrap()
            .query_wasm_smart(fractionalizer_address.clone(), &msg)
            .unwrap();

        let msg = Cw20QueryMsg::Balance {
            address: user_one.sender.clone().to_string(),
        };
        let res: cw20::BalanceResponse = router
            .wrap()
            .query_wasm_smart(address_res.address.clone(), &msg)
            .unwrap();
        assert_eq!(res.balance, Uint128::from(1u128));
        let msg = Cw20QueryMsg::Balance {
            address: user_two.sender.clone().to_string(),
        };
        let res: cw20::BalanceResponse = router
            .wrap()
            .query_wasm_smart(address_res.address.clone(), &msg)
            .unwrap();
        assert_eq!(res.balance, Uint128::from(2u128));
    }
}
