use alloy::{
    hex::FromHex,
    network::{EthereumWallet, ReceiptResponse, TransactionBuilder},
    primitives::{utils::parse_ether, Address, Bytes, U256},
    providers::{Provider, ProviderBuilder},
    rpc::types::eth::{
        state::{AccountOverride, StateOverride},
        TransactionRequest,
    },
    sol,
    sol_types::{SolCall, SolInterface},
    transports::Transport,
};
use alloy::rpc::types::TransactionReceipt;
use eyre::{bail, Context, ContextCompat, OptionExt};
use owo_colors::OwoColorize;

use crate::{
    config::Deploy,
    constants::ARB_WASM_ADDRESS,
    formatting::{format_data_fee, format_file_size, format_gas},
    wasm,
};

sol! {
    #[sol(rpc)]
    interface ArbWasm {
        function activateProgram(address program)
            external
            payable
            returns (uint16 version, uint256 dataFee);

        error ProgramNotWasm();
        error ProgramNotActivated();
        error ProgramNeedsUpgrade(uint16 version, uint16 stylusVersion);
        error ProgramExpired(uint64 ageInSeconds);
        error ProgramUpToDate();
        error ProgramKeepaliveTooSoon(uint64 ageInSeconds);
        error ProgramInsufficientValue(uint256 have, uint256 want);
    }
}

pub enum Status {
    Created(U256),
    Activated,
}

fn get_data_fee(fee: U256) -> U256 {
    // Give some leeway so that activation doesn't fail -- it'll get refunded
    // anyways.
    fee * U256::from(120) / U256::from(100)
}

pub async fn deploy(config: &Deploy) -> eyre::Result<TransactionReceipt> {
    let signer = config.auth.wallet()?;
    let sender = signer.address();

    let rpc_url = config.endpoint.parse()?;
    let provider = ProviderBuilder::new()
        .with_recommended_fillers()
        .wallet(EthereumWallet::from(signer))
        .on_http(rpc_url);

    let wasm_path = &config.generate_config.wasm;
    let legacy = config.generate_config.legacy;
    let runtime = wasm::compress(wasm_path, legacy).wrap_err("failed to compress wasm")?;

    let status = get_activation_fee(&runtime, &provider, sender).await?;
    if let Status::Created(fee) = status {
        if !config.quiet {
            println!("{:?}", fee);
        }
    }

    if !config.deploy_only {
        if let Status::Created(fee) = status {
            let data_fee = get_data_fee(fee);
            let visual_fee = format_data_fee(fee).unwrap_or("???".red().to_string());
            if !config.quiet {
                println!("wasm data fee: {}", visual_fee);
            }

            let balance = provider.get_balance(sender).await?;
            if balance < data_fee {
                bail!(
                    "not enough funds in account {} to pay for data fee\n\
                 balance {} < {}\n",
                    sender.red(),
                    balance.red(),
                    format!("{data_fee} wei").red(),
                );
            }
        }
    }

    let asm = crate::generate(&config.generate_config)?;
    if !config.quiet {
        println!("init code size: {}", format_file_size(asm.len(), 20, 28));
        println!("deploying to RPC: {}", &config.endpoint.bright_magenta());
    }

    let tx = TransactionRequest::default().into_create().with_input(asm);
    let receipt = provider.send_transaction(tx).await?.get_receipt().await?;
    let program = receipt
        .contract_address()
        .wrap_err("failed to read contract address from tx receipt")?;

    if !config.quiet {
        println!("deployed code: {}", program.bright_purple());
        println!(
            "deployment tx hash: {}",
            receipt.transaction_hash.bright_magenta()
        );
    }

    if !config.deploy_only {
        if let Status::Created(fee) = status {
            // Give some leeway so that activation doesn't fail -- it'll get refunded
            // anyways.
            let data_fee = get_data_fee(fee);
            let tx_input = ArbWasm::activateProgramCall { program }.abi_encode();
            let tx = TransactionRequest::default()
                .with_from(sender)
                .with_to(ARB_WASM_ADDRESS)
                .with_input(tx_input)
                .with_value(data_fee);

            if is_activated(&tx, &provider, &Default::default()).await? {
                if !config.quiet {
                    println!("{}", "wasm already activated!".bright_green());
                }
                return Ok(receipt);
            }

            if !config.quiet {
                println!("activating contract: {}", program);
            }
            let receipt = provider.send_transaction(tx).await?.get_receipt().await?;

            let gas = format_gas(U256::from(receipt.gas_used));
            if !config.quiet {
                println!("activated with {gas}");
                println!(
                    "activation tx hash: {}",
                    receipt.transaction_hash.bright_magenta()
                );
            }
        }
    }

    Ok(receipt)
}

async fn get_activation_fee<P, T>(
    runtime: &[u8],
    provider: &P,
    sender: Address,
) -> eyre::Result<Status>
where
    P: Provider<T>,
    T: Transport + Clone,
{
    let program = Address::random();
    let account_override = AccountOverride {
        code: Some(Bytes::copy_from_slice(runtime)),
        ..Default::default()
    };
    let mut overrides = StateOverride::default();
    overrides.insert(program, account_override);

    let sender_override = AccountOverride {
        balance: Some(U256::MAX),
        ..Default::default()
    };
    overrides.insert(sender, sender_override);

    let tx_input = ArbWasm::activateProgramCall { program }.abi_encode();
    let tx = TransactionRequest::default()
        .with_from(sender)
        .with_to(ARB_WASM_ADDRESS)
        .with_input(tx_input)
        .with_value(parse_ether("1").unwrap());

    if is_activated(&tx, &provider, &overrides).await? {
        return Ok(Status::Activated);
    }

    let output = provider.call(&tx).overrides(&overrides).await?;
    let ArbWasm::activateProgramReturn { dataFee, .. } =
        ArbWasm::activateProgramCall::abi_decode_returns(&output, true)?;

    Ok(Status::Created(dataFee))
}

async fn is_activated<P, T>(
    tx: &TransactionRequest,
    provider: &P,
    overrides: &StateOverride,
) -> eyre::Result<bool>
where
    P: Provider<T>,
    T: Transport + Clone,
{
    match provider.call(tx).overrides(overrides).await {
        Ok(_) => Ok(false),
        Err(e) => {
            let raw_value = e
                .as_error_resp()
                .map(|payload| payload.data.clone())
                .flatten()
                .ok_or_eyre(format!("{e}"))
                .wrap_err("could not check if the contract is activated")?;
            let bytes: [u8; 4] = FromHex::from_hex(raw_value.get().trim_matches('"'))?;

            use ArbWasm::ArbWasmErrors as Errors;
            match Errors::abi_decode(&bytes, true).wrap_err("unknown ArbWasm error")? {
                Errors::ProgramExpired(_) => Ok(false),
                Errors::ProgramNotWasm(_) => bail!("not a Stylus program"),
                Errors::ProgramUpToDate(_) => Ok(true),
                Errors::ProgramNotActivated(_) => Ok(false),
                Errors::ProgramNeedsUpgrade(_) => Ok(false),
                Errors::ProgramKeepaliveTooSoon(_) => bail!("unexpected ArbWasm error"),
                Errors::ProgramInsufficientValue(_) => bail!("unexpected ArbWasm error"),
            }
        }
    }
}
