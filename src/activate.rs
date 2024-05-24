use alloy::{
    contract::Error,
    hex::FromHex,
    network::{Network, TransactionBuilder},
    primitives::{keccak256, utils::parse_ether, Address, Bytes, B256, U256},
    providers::Provider,
    rpc::types::eth::{
        state::{AccountOverride, StateOverride},
        TransactionRequest,
    },
    sol,
    sol_types::{SolCall, SolInterface},
    transports::Transport,
};
use eyre::{bail, Result, WrapErr};
use owo_colors::OwoColorize;

use crate::{config::Deploy, constants::ARB_WASM_ADDRESS, formatting::format_data_fee, wasm};

/// Whether a program is active, or needs activation.
#[derive(PartialEq)]
pub enum ProgramStatus {
    /// Program already exists onchain.
    Active(Vec<u8>),
    /// Program can be activated with the given data fee.
    Ready(Vec<u8>, U256),
}

impl ProgramStatus {
    pub fn suggest_fee(&self) -> U256 {
        match self {
            Self::Active(_) => U256::default(),
            Self::Ready(_, data_fee) => data_fee * U256::from(120) / U256::from(100),
        }
    }
}

sol! {
    #[sol(rpc)]
    interface ArbWasm {
        function activateProgram(address program)
            external
            payable
            returns (uint16 version, uint256 dataFee);

        function stylusVersion() external view returns (uint16 version);

        function codehashVersion(bytes32 codehash) external view returns (uint16 version);

        error ProgramNotWasm();
        error ProgramNotActivated();
        error ProgramNeedsUpgrade(uint16 version, uint16 stylusVersion);
        error ProgramExpired(uint64 ageInSeconds);
        error ProgramUpToDate();
        error ProgramKeepaliveTooSoon(uint64 ageInSeconds);
        error ProgramInsufficientValue(uint256 have, uint256 want);
    }
}

pub async fn can_activate<T, P>(config: &Deploy, provider: &P) -> eyre::Result<ProgramStatus>
where
    P: Provider<T>,
    T: Transport + Clone,
{
    if config.endpoint == "https://stylus-testnet.arbitrum.io/rpc" {
        bail!(
            "The old Stylus testnet is no longer supported.\nPlease downgrade to {}",
            // format!("cargo stylus version 0.2.1").red()
            format!("cargo stylus version 0.2.1")
        );
    }

    let wasm_path = &config.generate_config.wasm;
    let runtime = wasm::compress(wasm_path).wrap_err("failed to compress WASM")?;
    let codehash = keccak256(&runtime);

    if program_exists(codehash, &provider).await? {
        return Ok(ProgramStatus::Active(runtime));
    }

    let address = Address::random();
    let sender = config.auth.wallet()?.address();
    let fee = get_activation_fee(&runtime, address, &provider, sender).await?;

    let visual_fee = format_data_fee(fee).unwrap_or("???".red().to_string());
    println!("wasm data fee: {}", visual_fee);

    Ok(ProgramStatus::Ready(runtime, fee))
}

/// Checks whether a program has already been activated with the most recent version of Stylus.
async fn program_exists<P, T, N>(codehash: B256, provider: &P) -> eyre::Result<bool>
where
    P: Provider<T, N>,
    T: Transport + Clone,
    N: Network,
{
    let arb_wasm = ArbWasm::new(ARB_WASM_ADDRESS, provider);
    let output = arb_wasm.codehashVersion(codehash).call_raw().await;

    match output {
        Ok(bytes) => {
            let ArbWasm::codehashVersionReturn { version } =
                ArbWasm::codehashVersionCall::abi_decode_returns(&bytes, true)?;
            let ArbWasm::stylusVersionReturn { version: v } =
                arb_wasm.stylusVersion().call().await?;

            return Ok(v == version);
        }
        Err(e) => {
            if let Error::TransportError(ref e) = e {
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
                    _ => bail!("unexpected ArbWasm error"),
                }
            }

            bail!("unexpected ArbWasm error")
        }
    }
}

/// Checks program activation, returning the data fee.
async fn get_activation_fee<P, T>(
    code: &[u8],
    address: Address,
    provider: &P,
    sender: Address,
) -> Result<U256>
where
    P: Provider<T>,
    T: Transport + Clone,
{
    let mut account_override = AccountOverride::default();
    account_override.code = Some(Bytes::copy_from_slice(code));
    let mut overrides = StateOverride::default();
    overrides.insert(address, account_override);

    let tx_input = ArbWasm::activateProgramCall { program: address }.abi_encode();
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
