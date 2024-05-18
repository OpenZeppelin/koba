use std::{mem::size_of, ops::Shr, path::Path, process::Command};

use eyre::{bail, OptionExt};

// PUSH32 (1 byte) + 32 bytes
const SHIFT_RIGHT: usize = 33;

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

pub fn amend(binary: Binary, wasm_length: usize, args_length: usize) -> eyre::Result<Binary> {
    let prelude = binary.prelude.clone();
    let prelude_push_instruction = get_push_with(prelude.len());
    let runtime_push_instruction = get_push_with(binary.code.len());
    let binary_push_instruction = get_push_with(prelude.len() + binary.code.len());

    let prelude_indices = find_all_instructions(&prelude, &prelude_push_instruction);
    if prelude_indices.len() == 0 {
        bail!("constructor bytecode is malformed: did not find prelude length");
    }
    let runtime_indices = find_all_instructions(&prelude, &runtime_push_instruction);
    if runtime_indices.len() == 0 {
        bail!("constructor bytecode is malformed: did not find runtime length");
    }
    let binary_indices = find_all_instructions(&prelude, &binary_push_instruction);

    let prelude_instructions_size = prelude_push_instruction.len() * prelude_indices.len();
    let runtime_instructions_size = runtime_push_instruction.len() * runtime_indices.len();
    let binary_instructions_size = binary_push_instruction.len() * binary_indices.len();
    let shift_left =
        prelude_instructions_size + runtime_instructions_size + binary_instructions_size;

    let total_indices = prelude_indices.len() + runtime_indices.len() + binary_indices.len();
    let new_prelude_length = prelude.len() - shift_left + SHIFT_RIGHT * total_indices;

    let prelude = amend_prelude(
        prelude,
        &prelude_indices,
        &prelude_push_instruction,
        new_prelude_length,
    );
    let prelude = amend_prelude(
        prelude,
        &runtime_indices,
        &runtime_push_instruction,
        wasm_length,
    );
    let prelude = amend_prelude(
        prelude,
        &binary_indices,
        &binary_push_instruction,
        new_prelude_length + wasm_length + args_length,
    );

    Ok(Binary {
        prelude,
        code: binary.code,
    })
}

fn get_push_with(length: usize) -> Vec<u8> {
    let byte_count = filled_bytes(length);
    let push0_opcode = 95;
    let push_opcode = push0_opcode + byte_count;

    let byte_count = byte_count as usize;
    let length_bytes = length.to_be_bytes();
    let start_byte = length_bytes.len() - byte_count;
    let mut push_instruction = vec![0u8; byte_count + 1];
    push_instruction[0] = push_opcode;
    push_instruction[1..].copy_from_slice(&length_bytes[start_byte..]);
    push_instruction
}

fn find_all_instructions(prelude: &[u8], instruction: &[u8]) -> Vec<usize> {
    prelude
        .windows(instruction.len())
        .enumerate()
        .filter(|(_, w)| *w == instruction)
        .map(|(offset, _)| offset + instruction.len())
        .collect::<Vec<_>>()
}

fn amend_prelude(
    mut prelude: Vec<u8>,
    indices: &[usize],
    instruction: &[u8],
    length: usize,
) -> Vec<u8> {
    for (idx, offset) in indices.into_iter().enumerate() {
        // Take into account shifts from previous prelude modifications.
        let offset = offset + idx * SHIFT_RIGHT;
        let suffix = prelude.split_off(offset);
        let prefix_length = prelude.len() - instruction.len();
        prelude = prelude.into_iter().take(prefix_length).collect::<Vec<u8>>();
        prelude.push(0x7f); // PUSH32
        prelude.extend(bytes32_from_usize(length));
        prelude.extend(&suffix);
    }

    prelude
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

#[cfg(test)]
mod tests {

    use super::{amend, Binary};

    #[test]
    fn amends_ctr_no_arguments() {
        let prelude =
            hex::decode("6080604052348015600e575f80fd5b5060055f55603e80601e5f395ff3fe").unwrap();
        let runtime = hex::decode("60806040525f80fdfea26469706673582212201930c24a9bacd514d80b227d7f262dc1678997d986b05b354f9d092c2fcaee5864736f6c63430008150033").unwrap();
        let binary = Binary {
            prelude,
            code: runtime,
        };

        let binary = amend(binary, 3388, 0).unwrap();
        let expected = hex::decode("6080604052348015600e575f80fd5b5060055f557f0000000000000000000000000000000000000000000000000000000000000d3c807f000000000000000000000000000000000000000000000000000000000000005c5f395ff3fe").unwrap();
        assert_eq!(expected, binary.prelude);
    }
}
