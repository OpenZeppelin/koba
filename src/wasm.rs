use std::{
    fs,
    io::{Cursor, Read},
    path::Path,
};

use brotli2::read::BrotliEncoder;
use eyre::Context;

pub const COMPRESSION_LEVEL: u32 = 11;
pub const EOF_PREFIX: &str = "EFF00000";

/// Reads a webassembly file at the specified `path` and attempts to compress
/// it.
pub fn compress(path: impl AsRef<Path>) -> eyre::Result<Vec<u8>> {
    let path = path.as_ref();
    let wasm = fs::read(path)
        .wrap_err_with(|| eyre::eyre!("failed to read wasm {}", path.to_string_lossy()))?;
    let wasm = wasmer::wat2wasm(&wasm).wrap_err("failed to parse wasm")?;

    let stream = Cursor::new(wasm);
    let mut compressor = BrotliEncoder::new(stream, COMPRESSION_LEVEL);
    let mut contract_code = hex::decode(EOF_PREFIX).unwrap();
    compressor
        .read_to_end(&mut contract_code)
        .wrap_err("failed to compress wasm bytes")?;

    Ok(contract_code)
}
