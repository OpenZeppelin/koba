mod activate;
mod assembler;
pub mod config;
mod constants;
mod deployer;
mod formatting;
mod solidity;
mod wallet;
mod wasm;

pub use config::run;
use config::Generate;
pub use deployer::deploy;
use eyre::Context;

impl Generate {
    pub fn run(&self) -> eyre::Result<()> {
        let generated = self.generate()?;
        let generated = hex::encode(generated);
        println!("{generated}");
        Ok(())
    }

    fn args(&self) -> eyre::Result<Vec<u8>> {
        Ok(self
            .args
            .iter()
            .map(hex::decode)
            .collect::<Result<Vec<_>, _>>()
            .wrap_err("args were not proper hex strings")?
            .concat())
    }

    pub fn generate(&self) -> eyre::Result<Vec<u8>> {
        let evmasm = solidity::assembly(&self.sol)?;
        let wasm = wasm::compress(&self.wasm)?;
        let asm = assembler::assemble(&evmasm, &wasm)?;
        let args = self.args()?;

        Ok([asm, args].concat())
    }
}
