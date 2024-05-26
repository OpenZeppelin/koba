use eyre::Context;

use crate::{assembler, config::Generate, solidity, wasm};

/// Generate deployment transaction data for a Stylus contract.
pub fn generate(config: &Generate) -> eyre::Result<Vec<u8>> {
    config.generate()
}

impl Generate {
    pub fn run(&self) -> eyre::Result<()> {
        let generated = self.generate()?;
        let generated = hex::encode(generated);
        println!("{generated}");
        Ok(())
    }

    fn args(&self) -> eyre::Result<Vec<u8>> {
        self.args
            .clone()
            .map_or(Ok(vec![]), hex::decode)
            .wrap_err("args were not proper hex strings")
    }

    fn generate(&self) -> eyre::Result<Vec<u8>> {
        let evmasm = solidity::assembly(&self.sol)?;
        let wasm = wasm::compress(&self.wasm)?;
        let asm = assembler::assemble(&evmasm, &wasm)?;
        let args = self.args()?;

        Ok([asm, args].concat())
    }
}
