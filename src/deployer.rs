use alloy::{
    network::{EthereumSigner, ReceiptResponse, TransactionBuilder},
    primitives::U256,
    providers::{Provider, ProviderBuilder},
    rpc::types::eth::TransactionRequest,
    sol,
    sol_types::SolCall,
};
use eyre::{bail, ContextCompat};
use owo_colors::OwoColorize;
use tokio::runtime::Builder;

use crate::{
    activate::{can_activate, ProgramStatus},
    config::Deploy,
    constants::ARB_WASM_ADDRESS,
    formatting::format_gas,
};

sol! {
    #[sol(rpc)]
    interface ArbWasm {
        function activateProgram(address program)
            external
            payable
            returns (uint16 version, uint256 dataFee);
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

    let program_status = can_activate(&config, &provider).await?;
    let data_fee = program_status.suggest_fee();
    if let ProgramStatus::Ready(..) = &program_status {
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

    let asm = config.generate_config.generate()?;
    let tx = TransactionRequest::default().into_create().with_input(asm);
    let receipt = provider.send_transaction(tx).await?.get_receipt().await?;
    let program = receipt
        .contract_address()
        .wrap_err("failed to read contract address from tx receipt")?;

    println!("deployed code: {}", program.bright_purple());

    match program_status {
        ProgramStatus::Ready(..) => {
            let tx_input = ArbWasm::activateProgramCall { program }.abi_encode();
            let tx = TransactionRequest::default()
                .with_from(sender)
                .with_to(ARB_WASM_ADDRESS)
                .with_input(tx_input)
                .with_value(data_fee);
            let receipt = provider.send_transaction(tx).await?.get_receipt().await?;

            let gas = format_gas(U256::from(receipt.gas_used));
            println!("activated with {gas}");
            println!(
                "ready onchain: {}",
                receipt.transaction_hash.bright_magenta()
            );
        }
        ProgramStatus::Active(_) => println!("wasm already activated!"),
    }

    Ok(())
}
