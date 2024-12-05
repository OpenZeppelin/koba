use alloy::primitives::U256;
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
        // User intends to deploy without constructor.
        if self.sol.is_none() {
            return self.plain_codegen();
        }

        let evmasm = solidity::assembly(self.sol.clone().unwrap())?;
        let wasm = wasm::compress(&self.wasm, self.legacy)?;
        let asm = assembler::assemble(&evmasm, &wasm)?;
        let args = self.args()?;

        Ok([asm, args].concat())
    }

    fn plain_codegen(&self) -> eyre::Result<Vec<u8>> {
        let wasm = wasm::compress(&self.wasm, self.legacy)?;

        let mut init_code = Vec::with_capacity(42 + wasm.len());
        init_code.push(0x7f); // PUSH32
        init_code.extend(U256::from(wasm.len()).to_be_bytes::<32>());
        init_code.push(0x80); // DUP1
        init_code.push(0x60); // PUSH1
        init_code.push(0x2a); // 42 the prelude length
        init_code.push(0x60); // PUSH1
        init_code.push(0x00);
        init_code.push(0x39); // CODECOPY
        init_code.push(0x60); // PUSH1
        init_code.push(0x00);
        init_code.push(0xf3); // RETURN
        init_code.extend(wasm);

        Ok(init_code)
    }
}
