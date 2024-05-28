# koba (工場)

Generate deployment transaction data for Stylus contracts.

> [!WARNING]
> This project is still in a very early and experimental phase. It has never
> been audited nor thoroughly reviewed for security vulnerabilities. Do not use
> in production.
>
> This project is meant to be temporary. The problem it solves should be fixed
> by either `cargo-stylus` itself or in the Stylus VM. As such, we maintain this
> on a best-effort basis.

## Installation

To install `koba` on your machine, just run `cargo install koba`. Compiling
Solidity code with `koba` requires `solc` to be installed and available through
the command line.

You can also use `koba` as a library by adding it to your project using
`cargo add koba`.

## Usage

You can use the command-line interface in two ways: the `generate` and the
`deploy` commands.

### `koba generate`

For a contract like this:

```rust
sol_storage! {
    #[entrypoint]
    pub struct Counter {
        uint256 number;
    }
}

#[external]
impl Counter {
    pub fn number(&self) -> U256 {
        self.number.get()
    }

    pub fn increment(&mut self) {
        let number = self.number.get();
        self.set_number(number + U256::from(1));
    }
}
```

With a constructor like this:

```solidity
contract Counter {
    uint256 private _number;

    constructor() {
        _number = 5;
    }
}
```

the following command outputs the transaction data you would need to send to
deploy the contract.

```sh
$ koba generate --sol <path-to-constructor> --wasm <path-to-wasm>
6080604052348015600e575f80fd5b...d81f197cb0f070175cce2fd57095700201
```

You can then use `cast` for example to deploy and activate the contract, like
this:

```sh
# Deploy the contract.
cast send --rpc-url https://stylusv2.arbitrum.io/rpc --private-key <private-key> --create <koba output>

# Activate the contract.
cast send --rpc-url https://stylusv2.arbitrum.io/rpc --private-key <private-key> --value "0.0001ether" 0x0000000000000000000000000000000000000071 "activateProgram(address)(uint16,uint256)" <contract address>

# Interact with the contract
cast call --rpc-url https://stylusv2.arbitrum.io/rpc <contract address> "number()"
0x0000000000000000000000000000000000000000000000000000000000000005

cast send --rpc-url https://stylusv2.arbitrum.io/rpc --private-key <private-key> <contract address> "increment()"

cast storage --rpc-url https://stylusv2.arbitrum.io/rpc <contract address> 0
0x0000000000000000000000000000000000000000000000000000000000000006
```

### `koba deploy`

For the same code in the above section, you can instead just run `koba deploy`
with the appropriate arguments to deploy and activate your Stylus contract in
one go:

```sh
$ koba deploy --sol <path-to-constructor> --wasm <path-to-wasm> --args <constructor-arguments> -e https://stylusv2.arbitrum.io/rpc --private-key <private-key>
wasm data fee: Ξ0.000113
init code size: 20.8 KB
deploying to RPC: https://stylusv2.arbitrum.io/rpc
deployed code: 0x470AE56DFbea924722423926782D8aB30f108A49
deployment tx hash: 0xb52a68b973fb883dbef6bf3e0cbee4f02608ae71ad5a89f6a2f0c9f094242a5b
activated with 2987042 gas
activation tx hash: 0x40086445e80365b648621fd62d978d716708fe05144f303baa620086eda854d1
success!
```

## Limitations

- `immutable` variables - `koba` currently does not support Solidity's
  `immutable` variables, since there is no equivalent mechanism for Stylus.
- `MCOPY` - Version [`0.8.24`][mcopy] of Solidity introduced the `MCOPY` opcode
  from `EIP-5656`. As of 2024-05-28, `nitro-testnode` does not support this
  opcode.

[mcopy]: https://github.com/ethereum/solidity/releases/tag/v0.8.24

## Why koba

`koba` means [factory](https://jisho.org/search/%E5%B7%A5%E5%A0%B4) in japanese
-- the factory where a stylus gets assembled.
