use alloy::{
    hex::FromHex,
    network::{EthereumSigner, ReceiptResponse, TransactionBuilder},
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
use eyre::{bail, Context, ContextCompat};
use owo_colors::OwoColorize;
use tokio::runtime::Builder;

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

pub fn deploy(config: &Deploy) -> eyre::Result<()> {
    let runtime = Builder::new_multi_thread().enable_all().build()?;
    runtime.block_on(deploy_impl(config))
}

async fn deploy_impl(config: &Deploy) -> eyre::Result<()> {
    let signer = config.auth.wallet()?;
    let sender = signer.address();

    let rpc_url = config.endpoint.parse()?;
    let provider = ProviderBuilder::new()
        .with_recommended_fillers()
        .signer(EthereumSigner::from(signer))
        .on_http(rpc_url);

    let wasm_path = &config.generate_config.wasm;
    let runtime = wasm::compress(wasm_path).wrap_err("failed to compress wasm")?;
    let fee = get_activation_fee(&runtime, &provider, sender).await?;

    // Give some leeway so that activation doesn't fail -- it'll get refunded
    // anyways.
    let data_fee = fee * U256::from(120) / U256::from(100);
    let visual_fee = format_data_fee(fee).unwrap_or("???".red().to_string());
    println!("wasm data fee: {}", visual_fee);

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

    let asm = crate::generate(&config.generate_config)?;
    println!("init code size: {}", format_file_size(asm.len(), 20, 28));
    println!("deploying to RPC: {}", &config.endpoint.bright_magenta());

    let tx = TransactionRequest::default().into_create().with_input(asm);
    let receipt = provider.send_transaction(tx).await?.get_receipt().await?;
    let program = receipt
        .contract_address()
        .wrap_err("failed to read contract address from tx receipt")?;

    println!("deployed code: {}", program.bright_purple());
    println!(
        "deployment tx hash: {}",
        receipt.transaction_hash.bright_magenta()
    );

    let tx_input = ArbWasm::activateProgramCall { program }.abi_encode();
    let tx = TransactionRequest::default()
        .with_from(sender)
        .with_to(ARB_WASM_ADDRESS)
        .with_input(tx_input)
        .with_value(data_fee);

    if is_activated(&tx, &provider).await? {
        println!("{}", "wasm already activated!".bright_green());
        return Ok(());
    }

    let receipt = provider.send_transaction(tx).await?.get_receipt().await?;

    let gas = format_gas(U256::from(receipt.gas_used));
    println!("activated with {gas}");
    println!(
        "activation tx hash: {}",
        receipt.transaction_hash.bright_magenta()
    );

    Ok(())
}

async fn get_activation_fee<P, T>(
    runtime: &[u8],
    provider: &P,
    sender: Address,
) -> eyre::Result<U256>
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

    let tx_input = ArbWasm::activateProgramCall { program }.abi_encode();
    let tx = TransactionRequest::default()
        .with_from(sender)
        .with_to(ARB_WASM_ADDRESS)
        .with_input(tx_input)
        .with_value(parse_ether("1").unwrap());

    let output = provider.call(&tx).overrides(&overrides).await?;
    let ArbWasm::activateProgramReturn { dataFee, .. } =
        ArbWasm::activateProgramCall::abi_decode_returns(&output, true)?;

    Ok(dataFee)
}

async fn is_activated<P, T>(tx: &TransactionRequest, provider: &P) -> eyre::Result<bool>
where
    P: Provider<T>,
    T: Transport + Clone,
{
    let output = provider.call(tx).await;
    if let Err(e) = output {
        let Some(payload) = e.as_error_resp() else {
            bail!("transport error {e}");
        };
        let Some(ref raw_value) = payload.data else {
            bail!("transport error {e}");
        };
        let bytes = raw_value.get();
        let bytes: [u8; 4] = FromHex::from_hex(bytes.trim_matches('"'))?;

        use ArbWasm::ArbWasmErrors as Errors;
        let error = Errors::abi_decode(&bytes, true).wrap_err("unknown ArbWasm error")?;
        match error {
            Errors::ProgramNotWasm(_) => bail!("not a Stylus program"),
            Errors::ProgramNotActivated(_)
            | Errors::ProgramNeedsUpgrade(_)
            | Errors::ProgramExpired(_) => {
                return Ok(false);
            }
            Errors::ProgramUpToDate(_) => {
                return Ok(true);
            }
            Errors::ProgramKeepaliveTooSoon(_) | Errors::ProgramInsufficientValue(_) => {
                bail!("unexpected ArbWasm error");
            }
        };
    }

    Ok(false)
}
