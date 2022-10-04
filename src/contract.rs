use crate::response::MsgInstantiateContractResponse;
#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    from_binary, to_binary, Addr, Binary, CosmosMsg, Deps, DepsMut, Empty, Env, MessageInfo, Reply,
    ReplyOn, Response, StdError, StdResult, SubMsg, Uint128, WasmMsg,
};
use cw2::set_contract_version;
use cw20::{Cw20ReceiveMsg, Cw20Coin};
use cw721::Cw721ReceiveMsg;
use protobuf::Message;

use crate::error::ContractError;
use crate::msg::{ExecuteMsg, GetCw20AddressResponse, InstantiateMsg, QueryMsg, ReceiveMsg};
use crate::state::{Config, CONFIG, CW20_NFT, NFT_CW20};
use cw20_base::msg::{ExecuteMsg as Cw20ExecuteMsg, InstantiateMsg as Cw20InstantiateMsg};
use cw721_base::msg::ExecuteMsg as Cw721ExecuteMsg;

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

pub fn handle_unfractionalize(
    deps: DepsMut,
    info: MessageInfo,
    env: Env,
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
        ReceiveMsg::Fractionalize { owners } => {
            fractionalize(deps, info.sender, wrapped.token_id, owners)
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

#[cfg(test)]
mod tests {
    use super::*;
    use cosmwasm_std::{
        testing::{mock_dependencies, mock_env, mock_info, MockApi, MockQuerier},
        Addr, Api, Empty, MemoryStorage, OwnedDeps,
    };
    use cw20::Cw20QueryMsg;
    use cw721::Cw721QueryMsg;
    use cw721::{NumTokensResponse, OwnerOfResponse};
    use cw721_base::{Extension, MintMsg, InstantiateMsg as Cw721InstantiateMsg};
    use cw_multi_test::{App, AppResponse, BankKeeper, Contract, ContractWrapper, Executor};

    pub fn nft_owner_of(router: &mut App, collection: String, token_id: String) -> String {
        let msg = Cw721QueryMsg::OwnerOf {
            token_id,
            include_expired: None,
        };
        let res: OwnerOfResponse = router.wrap().query_wasm_smart(collection, &msg).unwrap();
        return res.owner;
    }

    pub fn token_balance(router: &mut App, token: String, user: String) -> Uint128 {
        let msg = Cw20QueryMsg::Balance {
            address: user.to_string(),
        };
        let res: cw20::BalanceResponse = router
            .wrap()
            .query_wasm_smart(token.to_string(), &msg)
            .unwrap();
        res.balance
    }

    pub fn token_transfer(
        router: &mut App,
        sender: Addr,
        token: Addr,
        amount: Uint128,
        recipient: Addr,
    ) {
        let msg = Cw20ExecuteMsg::Transfer {
            recipient: recipient.to_string(),
            amount,
        };
        let _ = router.execute_contract(sender, token, &msg, &[]).unwrap();
    }

    pub fn mint_nft(
        router: &mut App,
        minter: Addr,
        collection: Addr,
        token_id: String,
        user: Addr,
    ) {
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

    pub fn approve_nft(
        router: &mut App,
        approver: Addr,
        spender: Addr,
        collection: Addr,
        token_id: String,
    ) {
        let msg = Cw721ExecuteMsg::<Empty, Empty>::Approve {
            spender: spender.to_string(),
            token_id: token_id,
            expires: None,
        };
        let _ = router
            .execute_contract(approver, collection, &msg, &[])
            .unwrap();
    }

    pub fn get_fractional_address(
        router: &mut App,
        fractionalizer_address: Addr,
        collection: Addr,
        token_id: String,
    ) -> String {
        let cw20_address: GetCw20AddressResponse = router
            .wrap()
            .query_wasm_smart(
                fractionalizer_address,
                &QueryMsg::GetCw20Address {
                    address: collection.to_string(),
                    token_id: token_id.to_string(),
                },
            )
            .unwrap();
        return cw20_address.address;
    }

    pub fn fractionalize(
        router: &mut App,
        sender: Addr,
        fractionalizer_address: Addr,
        collection: Addr,
        token_id: String,
        owners: Vec<Cw20Coin>,
    ) {
        let msg = Cw721ExecuteMsg::<Empty, Empty>::SendNft {
            contract: fractionalizer_address.clone().to_string(),
            token_id: token_id.clone(),
            msg: to_binary(&ReceiveMsg::Fractionalize { owners }).unwrap(),
        };
        router
            .execute_contract(sender, collection, &msg, &[])
            .unwrap();
    }

    pub fn unfractionalize(
        router: &mut App,
        sender: Addr,
        fractionalizer_address: Addr,
        cw20_address: Addr,
        amount: Uint128,
    ) -> Result<AppResponse, anyhow::Error> {
        let msg = Cw20ExecuteMsg::Send {
            contract: fractionalizer_address.clone().to_string(),
            amount,
            msg: to_binary(&ReceiveMsg::Unfractionalize {
                recipient: sender.clone().to_string(),
            })
            .unwrap(),
        };

        router.execute_contract(sender.clone(), cw20_address.clone(), &msg, &[])
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
        let contract = ContractWrapper::new(execute, instantiate, query).with_reply(reply);
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

    fn setup(router: &mut App) -> World {
        let deployer = mock_info("deployer", &[]);

        let mut deps = mock_dependencies();

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
            .instantiate_contract(
                contract_code_id,
                deployer.clone().sender,
                &msg,
                &[],
                "counter",
                None,
            )
            .unwrap();

        // Fractionalizer
        let contract_code_id = router.store_code(contract_fractionalizer());
        let msg = InstantiateMsg {};
        let fractionalizer_address = router
            .instantiate_contract(
                contract_code_id,
                deployer.clone().sender,
                &msg,
                &[],
                "counter",
                None,
            )
            .unwrap();

        let nft_address = deps.api.addr_validate(&nft_address.to_string()).unwrap();

        return World {
            deps: deps,

            deployer_address: deployer.sender,
            fractionalizer_address: fractionalizer_address,
            nft_address,
            user_one: user_one.sender,
            user_two: user_two.sender,
        };
    }

    fn mock_app() -> App {
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
    }

    #[test]
    fn test_mint() {
        let router = &mut mock_app();
        let w = setup(router);

        let token_id = "nft".to_string();
        mint_nft(
            router,
            w.deployer_address.clone(),
            w.nft_address.clone(),
            token_id.clone(),
            w.deployer_address.clone(),
        );

        let owner_of = nft_owner_of(
            router,
            w.nft_address.clone().to_string(),
            token_id.to_string(),
        );
        assert_eq!(owner_of, w.deployer_address.clone().to_string());
    }

    #[test]
    fn test_fractionalize() {
        let router = &mut mock_app();
        let w = setup(router);

        // mint & approve NFT
        let token_id = "nft".to_string();
        mint_nft(
            router,
            w.deployer_address.clone(),
            w.nft_address.clone(),
            token_id.clone(),
            w.deployer_address.clone(),
        );

        approve_nft(
            router,
            w.deployer_address.clone(),
            w.fractionalizer_address.clone(),
            w.nft_address.clone(),
            token_id.clone(),
        );

        fractionalize(
            router,
            w.deployer_address.clone(),
            w.fractionalizer_address.clone(),
            w.nft_address.clone(),
            token_id.clone(),
            vec![
                Cw20Coin {
                    address: w.user_one.clone().to_string(),
                    amount: Uint128::from(1u128),
                },
                Cw20Coin {
                    address: w.user_two.clone().to_string(),
                    amount: Uint128::from(2u128),
                },
            ],
        );

        let cw20 = get_fractional_address(
            router,
            w.fractionalizer_address.clone(),
            w.nft_address.clone(),
            token_id.clone(),
        );

        // assertions
        let owner_of = nft_owner_of(
            router,
            w.nft_address.clone().to_string(),
            token_id.to_string(),
        );
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
        mint_nft(
            router,
            w.deployer_address.clone(),
            w.nft_address.clone(),
            token_id.clone(),
            w.deployer_address.clone(),
        );

        approve_nft(
            router,
            w.deployer_address.clone(),
            w.fractionalizer_address.clone(),
            w.nft_address.clone(),
            token_id.clone(),
        );

        fractionalize(
            router,
            w.deployer_address.clone(),
            w.fractionalizer_address.clone(),
            w.nft_address.clone(),
            token_id.clone(),
            vec![
                Cw20Coin {
                    address: w.user_one.clone().to_string(),
                    amount: Uint128::from(1u128),
                },
                Cw20Coin {
                    address: w.user_two.clone().to_string(),
                    amount: Uint128::from(2u128),
                },
            ],
        );

        let cw20 = get_fractional_address(
            router,
            w.fractionalizer_address.clone(),
            w.nft_address.clone(),
            token_id.clone(),
        );
        let cw20_address = w.deps.api.addr_validate(&cw20).unwrap();
        token_transfer(
            router,
            w.user_two.clone(),
            cw20_address.clone(),
            Uint128::from(2u128),
            w.user_one.clone(),
        );

        let bal = token_balance(router, cw20.clone(), w.user_one.clone().to_string());
        assert_eq!(bal, Uint128::from(3u128));

        let err = unfractionalize(
            router,
            w.user_one.clone(),
            w.fractionalizer_address.clone(),
            cw20_address.clone(),
            Uint128::from(1u128),
        )
        .unwrap_err();
        assert_eq!(
            err.downcast::<ContractError>().unwrap(),
            ContractError::InsufficientFunds {}
        );
        unfractionalize(
            router,
            w.user_one.clone(),
            w.fractionalizer_address.clone(),
            cw20_address.clone(),
            bal,
        )
        .unwrap();
        unfractionalize(
            router,
            w.user_one.clone(),
            w.fractionalizer_address.clone(),
            cw20_address,
            bal,
        )
        .unwrap_err();
        // TODO: figure out how to assert this error

        // assertions
        let owner_of = nft_owner_of(
            router,
            w.nft_address.to_string(),
            token_id,
        );
        assert_eq!(owner_of, w.user_one.clone().to_string());

        let bal = token_balance(router, cw20.clone(), w.user_one.to_string());
        assert_eq!(bal, Uint128::from(0u128));

        let bal = token_balance(router, cw20.clone(), w.user_two.to_string());
        assert_eq!(bal, Uint128::from(0u128));

        let bal = token_balance(router, cw20, w.fractionalizer_address.to_string());
        assert_eq!(bal, Uint128::from(0u128));
    }
}
