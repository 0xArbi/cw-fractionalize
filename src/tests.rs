use cosmwasm_std::{
    to_binary, Addr, Api, Empty, Uint128,
        testing::{mock_dependencies, mock_info, MockApi, MockQuerier},
    MemoryStorage, OwnedDeps,

};

use cw20::{Cw20Coin};
use cw20_base::msg::{ExecuteMsg as Cw20ExecuteMsg};
use cw721_base::msg::ExecuteMsg as Cw721ExecuteMsg;
use cw20::Cw20QueryMsg;
use cw721::Cw721QueryMsg;
use cw721::{NumTokensResponse, OwnerOfResponse};
use cw721_base::{Extension, InstantiateMsg as Cw721InstantiateMsg, MintMsg};
use cw_multi_test::{App, AppResponse, Contract, ContractWrapper, Executor};

use crate::contract::{execute, instantiate, query, reply};
use crate::error::ContractError;
use crate::msg::{GetCw20AddressResponse, InstantiateMsg, QueryMsg, ReceiveMsg};

pub fn nft_owner_of(router: &mut App, collection: String, token_id: String) -> String {
    let msg = Cw721QueryMsg::OwnerOf {
        token_id,
        include_expired: None,
    };
    let res: OwnerOfResponse = router.wrap().query_wasm_smart(collection, &msg).unwrap();
    res.owner
}

pub fn token_balance(router: &mut App, token: String, address: String) -> Uint128 {
    let msg = Cw20QueryMsg::Balance {
        address,
    };
    let res: cw20::BalanceResponse = router
        .wrap()
        .query_wasm_smart(token, &msg)
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

pub fn mint_nft(router: &mut App, minter: Addr, collection: Addr, token_id: String, user: Addr) {
    let mint_msg = Cw721ExecuteMsg::Mint::<_, Extension>(MintMsg::<Extension> {
        token_id,
        owner: user.to_string(),
        token_uri: Some("".to_string()),
        extension: None,
    });
    let _ = router
        .execute_contract(minter, collection, &mint_msg, &[])
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
        token_id,
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
                token_id,
            },
        )
        .unwrap();
    cw20_address.address
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
        contract: fractionalizer_address.to_string(),
        token_id,
        msg: to_binary(&ReceiveMsg::Fractionalize { 
            owners, 
            name: "name".to_string(), 
            symbol: "symbol".to_string(),
        }).unwrap(),
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
        contract: fractionalizer_address.to_string(),
        amount,
        msg: to_binary(&ReceiveMsg::Unfractionalize {
            recipient: sender.to_string(),
        })
        .unwrap(),
    };

    router.execute_contract(sender, cw20_address, &msg, &[])
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

    let deps = mock_dependencies();

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
            "nft",
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
            "fractionalizer",
            None,
        )
        .unwrap();

    let nft_address = deps.api.addr_validate(&nft_address.to_string()).unwrap();

    World {
        deps,

        deployer_address: deployer.sender,
        fractionalizer_address,
        nft_address,
        user_one: user_one.sender,
        user_two: user_two.sender,
    }
}

fn mock_app() -> App {
    App::new(|_a, _b, _c| {})
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
        w.nft_address.to_string(),
        token_id,
    );
    assert_eq!(owner_of, w.deployer_address.to_string());
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
                address: w.user_one.to_string(),
                amount: Uint128::from(1u128),
            },
            Cw20Coin {
                address: w.user_two.to_string(),
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
        w.nft_address.to_string(),
        token_id,
    );
    assert_eq!(owner_of, w.fractionalizer_address.to_string());

    let bal = token_balance(router, cw20.clone(), w.user_one.to_string());
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
                address: w.user_one.to_string(),
                amount: Uint128::from(1u128),
            },
            Cw20Coin {
                address: w.user_two.to_string(),
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

    let bal = token_balance(router, cw20.clone(), w.user_one.to_string());
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
    let owner_of = nft_owner_of(router, w.nft_address.to_string(), token_id);
    assert_eq!(owner_of, w.user_one.to_string());

    let bal = token_balance(router, cw20.clone(), w.user_one.to_string());
    assert_eq!(bal, Uint128::from(0u128));

    let bal = token_balance(router, cw20.clone(), w.user_two.to_string());
    assert_eq!(bal, Uint128::from(0u128));

    let bal = token_balance(router, cw20, w.fractionalizer_address.to_string());
    assert_eq!(bal, Uint128::from(0u128));
}
