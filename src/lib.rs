pub mod config;
pub use config::run;

mod assembler;
mod solidity;
mod wasm;

pub fn generate(config: &config::Generate) -> eyre::Result<()> {
    let evmasm = solidity::assembly(&config.sol)?;
    let wasm = wasm::compress(&config.wasm)?;
    let asm = assembler::compile(&evmasm, &wasm)?;

    let args = config
        .args
        .iter()
        .map(hex::decode)
        .collect::<Result<Vec<_>, _>>()?
        .concat();
    let args = hex::encode(args);

    let init_code = format!("{asm}{args}");
    println!("{}", init_code);

    Ok(())
}
