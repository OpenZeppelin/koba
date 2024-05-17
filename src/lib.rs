pub mod config;
pub use config::run;

mod solidity;
mod wasm;

pub fn generate(config: &config::Generate) -> eyre::Result<()> {
    let binary = solidity::compile(&config.sol)?;
    let wasm = wasm::compress(&config.wasm)?;
    let binary = solidity::amend(binary, wasm.len())?;
    let args = config
        .args
        .iter()
        .map(|a| hex::decode(a))
        .collect::<Result<Vec<_>, _>>()?
        .concat();

    let init_code = [binary.prelude, wasm, args].concat();

    println!("{}", hex::encode(init_code));

    Ok(())
}