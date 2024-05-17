use std::{mem::size_of, ops::Shr, path::Path, process::Command};

use eyre::{bail, OptionExt};

pub struct Binary {
    pub prelude: Vec<u8>,
    pub code: Vec<u8>,
}

enum BinaryKind {
    Full,
    Runtime,
}

pub fn compile(sol_path: impl AsRef<Path>) -> eyre::Result<Binary> {
    let path = sol_path.as_ref();
    let bin_output = run_solc(path, BinaryKind::Full)?;
    let bin_runtime_output = run_solc(path, BinaryKind::Runtime)?;

    let prelude = bin_output.replace(&bin_runtime_output, "");
    let prelude = hex::decode(prelude)?;
    let code = hex::decode(bin_runtime_output)?;
    Ok(Binary { prelude, code })
}

fn run_solc(file_path: impl AsRef<Path>, kind: BinaryKind) -> eyre::Result<String> {
    let binary_or_runtime_flag = match kind {
        BinaryKind::Full => "--bin",
        BinaryKind::Runtime => "--bin-runtime",
    };
    let output = Command::new("solc")
        .arg(binary_or_runtime_flag)
        .arg(file_path.as_ref())
        .arg("--optimize")
        .output()?;
    let output = String::from_utf8_lossy(&output.stdout);
    output
        .lines()
        .last()
        .ok_or_eyre("compiler output is empty")
        .map(|s| s.to_owned())
}

pub fn amend(binary: Binary, wasm_length: usize) -> eyre::Result<Binary> {
    let prelude_length = binary.prelude.len();
    let binary = amend_clen(binary, wasm_length)?;
    let binary = amend_plen(binary, prelude_length)?;

    Ok(binary)
}

fn amend_clen(binary: Binary, wasm_length: usize) -> eyre::Result<Binary> {
    let length = binary.code.len();
    let byte_count = filled_bytes(length);
    let push0_opcode = 95;
    let push_opcode = push0_opcode + byte_count;

    let byte_count = byte_count as usize;
    let length_bytes = length.to_be_bytes();
    let start_byte = length_bytes.len() - byte_count;
    let mut push_instruction = vec![0u8; byte_count + 1];
    push_instruction[0] = push_opcode;
    push_instruction[1..].copy_from_slice(&length_bytes[start_byte..]);

    let mut prelude = binary.prelude.clone();
    let indices = prelude
        .windows(push_instruction.len())
        .enumerate()
        .filter(|(_, w)| *w == push_instruction)
        .map(|(offset, _)| offset + push_instruction.len())
        .collect::<Vec<_>>();

    if indices.len() == 0 {
        bail!("constructor bytecode is malformed: did not find binary length");
    }

    // PUSH32 (1 byte) + 32 bytes - (PUSH1 (1 byte) + 1 byte).
    let shift = 31;
    for (idx, offset) in indices.into_iter().enumerate() {
        // Take into account shifts from previous prelude modifications.
        let offset = offset + idx * shift;
        let suffix = prelude.split_off(offset);
        let prefix_length = prelude.len() - push_instruction.len();
        prelude = prelude.into_iter().take(prefix_length).collect::<Vec<u8>>();
        prelude.push(0x7f); // PUSH32
        prelude.extend(bytes32_from_usize(wasm_length));
        prelude.extend(&suffix);
    }

    Ok(Binary {
        prelude,
        code: binary.code,
    })
}

fn amend_plen(binary: Binary, prelude_length: usize) -> eyre::Result<Binary> {
    let length = prelude_length;
    let byte_count = filled_bytes(length);
    let push0_opcode = 95;
    let push_opcode = push0_opcode + byte_count;

    let byte_count = byte_count as usize;
    let length_bytes = length.to_be_bytes();
    let start_byte = length_bytes.len() - byte_count;
    let mut push_instruction = vec![0u8; byte_count + 1];
    push_instruction[0] = push_opcode;
    push_instruction[1..].copy_from_slice(&length_bytes[start_byte..]);

    let mut prelude = binary.prelude.clone();
    let indices = prelude
        .windows(push_instruction.len())
        .enumerate()
        .filter(|(_, w)| *w == push_instruction)
        .map(|(offset, _)| offset + push_instruction.len())
        .collect::<Vec<_>>();

    if indices.len() == 0 {
        bail!("constructor bytecode is malformed: could not find prelude length");
    }

    // PUSH32 (1 byte) + 32 bytes - (PUSH1 (1 byte) + 1 byte).
    let shift = 31;
    let new_prefix_length =
        prelude.len() - push_instruction.len() * indices.len() + shift * indices.len();
    for (idx, offset) in indices.into_iter().enumerate() {
        // Take into account shifts from previous prelude modifications.
        let offset = offset + idx * shift;
        let suffix = prelude.split_off(offset);
        let prefix_length = prelude.len() - push_instruction.len();
        prelude = prelude.into_iter().take(prefix_length).collect::<Vec<u8>>();
        prelude.push(0x7f); // PUSH32
        prelude.extend(bytes32_from_usize(new_prefix_length));
        prelude.extend(&suffix);
    }

    Ok(Binary {
        prelude,
        code: binary.code,
    })
}

fn filled_bytes(mut n: usize) -> u8 {
    let mut count = 0;
    while n > 0 {
        n = n.shr(8);
        count += 1;
    }
    assert!(count <= 32);
    count
}

fn bytes32_from_usize(n: usize) -> [u8; 32] {
    let mut bytes = [0u8; 32];
    let size = size_of::<usize>();
    bytes[32 - size..].copy_from_slice(&n.to_be_bytes());
    bytes
}
