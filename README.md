# cw-fractionalize

`cw-fractionalize` is a permissionless, public good, CosmWasm contract for fractionalizing NFTs.

## Usage

Sending a CW721 compliant NFT to the contract will fractionalize its ownership via a freshly deployed CW20 contract. Initial token balances are specified by the sender.

To "unfractionalize" the NFT, all the CW20 tokens need to be sent back to the contract, which will then be subsequently burned.
