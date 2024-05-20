use std::{io::BufRead, mem::size_of, ops::Shr, path::Path, process::Command};

use eyre::{bail, OptionExt};

// PUSH32 (1 byte) + 32 bytes
const SHIFT_RIGHT: usize = 33;
const USIZE_BYTES: usize = size_of::<usize>();

const PUSH0_OPCODE: u8 = 0x5f;
const PUSH32_OPCODE: u8 = 0x7f;
const JUMP_OPCODE: u8 = 0x56;
const JUMPI_OPCODE: u8 = 0x57;

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

pub fn assembly(sol_path: impl AsRef<Path>) -> eyre::Result<String> {
    let output = Command::new("solc")
        .arg(sol_path.as_ref())
        .arg("--asm")
        .arg("--optimize")
        .output()?;
    let code = String::from_utf8_lossy(&output.stdout);
    let code = code
        .to_string()
        .lines()
        .skip_while(|l| !l.contains("EVM"))
        // Also skip the line containing `EVM`.
        .skip(1)
        .collect::<Vec<_>>()
        .join("\n");

    Ok(code)
}

pub fn amend(binary: Binary, wasm_length: usize) -> eyre::Result<Binary> {
    let prelude = binary.prelude.clone();
    let prelude_push_instruction = create_push_instruction(prelude.len());
    let runtime_push_instruction = create_push_instruction(binary.code.len());
    let binary_push_instruction = create_push_instruction(prelude.len() + binary.code.len());
    let jumps = get_jumps(&prelude);
    let jump_instructions = jumps
        .iter()
        .map(|jump| create_push_instruction(jump.dst))
        .collect::<Vec<_>>();
    let jump_shift_left: usize = jump_instructions
        .iter()
        .map(|instruction| instruction.len())
        .sum();

    let prelude_indices = get_instructions(&prelude, &prelude_push_instruction, &jumps);
    if prelude_indices.len() == 0 {
        bail!("constructor bytecode is malformed: did not find prelude length");
    }
    let runtime_indices = get_instructions(&prelude, &runtime_push_instruction, &jumps);
    if runtime_indices.len() == 0 {
        bail!("constructor bytecode is malformed: did not find runtime length");
    }
    let binary_indices = get_instructions(&prelude, &binary_push_instruction, &jumps);

    let prelude_instructions_size = prelude_push_instruction.len() * prelude_indices.len();
    let runtime_instructions_size = runtime_push_instruction.len() * runtime_indices.len();
    let binary_instructions_size = binary_push_instruction.len() * binary_indices.len();
    let shift_left = prelude_instructions_size
        + runtime_instructions_size
        + binary_instructions_size
        + jump_shift_left;
    let total_indices = prelude_indices.len()
        + runtime_indices.len()
        + binary_indices.len()
        + jump_instructions.len();
    let new_prelude_length = prelude.len() - shift_left + SHIFT_RIGHT * total_indices;
    let cp = get_cumulative_shift(
        &prelude_indices,
        prelude.len(),
        prelude_push_instruction.len(),
    );
    let cr = get_cumulative_shift(
        &runtime_indices,
        prelude.len(),
        runtime_push_instruction.len(),
    );
    let cb = get_cumulative_shift(
        &binary_indices,
        prelude.len(),
        binary_push_instruction.len(),
    );

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
        new_prelude_length + wasm_length,
    );

    let mut cumulative_shifts = cp
        .into_iter()
        .zip(cr)
        .zip(cb)
        .map(|((a, b), c)| a + b + c)
        .collect::<Vec<_>>();

    for jump in jumps.iter() {
        let offset = jump.push_offset;
        let size = jump.push_opcode - PUSH0_OPCODE as usize + 1;
        for i in offset + size..cumulative_shifts.len() {
            cumulative_shifts[i] += SHIFT_RIGHT - size;
        }
    }

    let prelude = amend_jumps(prelude, &jumps, cumulative_shifts);
    Ok(Binary {
        prelude,
        code: binary.code,
    })
}

fn get_cumulative_shift(indices: &[usize], prelude_length: usize, shift_left: usize) -> Vec<usize> {
    let mut shifts = vec![0usize; prelude_length];
    let index_count = indices.len();
    if index_count == 0 {
        return shifts;
    }

    let mut cs = 0;
    for i in 0..index_count - 1 {
        cs += SHIFT_RIGHT - shift_left;
        let left = indices[i];
        let right = indices[i + 1];
        shifts[left..right].copy_from_slice(&vec![cs; right - left]);
    }

    cs += SHIFT_RIGHT - shift_left;
    let suffix = vec![cs; prelude_length - indices[index_count - 1]];
    shifts[indices[index_count - 1]..].copy_from_slice(&suffix);
    shifts
}

/// Information about the sequence `PUSHX dst JUMP/JUMPI`.
#[derive(Debug, PartialEq)]
struct Jump {
    /// The offset of the PUSHX instruction preceding the jump.
    push_offset: usize,
    /// Which PUSHX instruction is preceding the jump. `usize` for convenience.
    push_opcode: usize,
    /// Argument passed to the preceding PUSHX instruction.
    ///
    /// `usize` should be enough to fit any init-code size.
    dst: usize,
}

/// Finds the offsets at which the sequence `PUSHX dst JUMP/JUMPI` starts.
fn get_jumps(bytecode: &[u8]) -> Vec<Jump> {
    let mut jumps = vec![];
    let mut i = 0;
    while i < bytecode.len() {
        let opcode = bytecode[i];

        // Check if opcode is a PUSHX.
        if PUSH0_OPCODE < opcode && opcode <= PUSH32_OPCODE {
            let opcode = opcode as usize;
            let j = i + opcode - PUSH0_OPCODE as usize + 1;
            // If next opcode is a jump, then we found the sequence we want.
            if bytecode[j] == JUMP_OPCODE || bytecode[j] == JUMPI_OPCODE {
                // `usize` should be enough to fit any init-code size.
                let dst = usize_from_bytes(&bytecode[i + 1..j]);
                jumps.push(Jump {
                    push_offset: i,
                    push_opcode: opcode,
                    dst,
                });
                i = j;
            }
        }

        i += 1;
    }

    jumps
}

fn create_push_instruction(arg: usize) -> Vec<u8> {
    let byte_count = filled_bytes(arg);
    let opcode = PUSH0_OPCODE + byte_count as u8;

    let mut instruction = vec![0u8; byte_count + 1];
    instruction[0] = opcode;

    if arg > 0 {
        let bytes = arg.to_be_bytes();
        let start = bytes.len() - byte_count;
        instruction[1..].copy_from_slice(&bytes[start..]);
    }

    instruction
}

fn get_instructions(prelude: &[u8], instruction: &[u8], jumps: &[Jump]) -> Vec<usize> {
    prelude
        .windows(instruction.len())
        .enumerate()
        .filter(|(_, w)| *w == instruction)
        .filter(|(offset, _)| !jumps.iter().any(|j| j.push_offset == *offset))
        .map(|(offset, _)| offset + instruction.len())
        .collect::<Vec<_>>()
}

fn amend_prelude(
    mut prelude: Vec<u8>,
    indices: &[usize],
    instruction: &[u8],
    length: usize,
) -> Vec<u8> {
    let size = instruction.len();
    for (idx, offset) in indices.into_iter().enumerate() {
        let mut instruction = vec![PUSH32_OPCODE];
        instruction.extend(bytes32_from_usize(length));
        let offset = offset + idx * SHIFT_RIGHT - (idx + 1) * size;
        prelude.splice(offset..offset + size, instruction);
    }

    prelude
}

fn amend_jumps(mut prelude: Vec<u8>, jumps: &[Jump], shifts: Vec<usize>) -> Vec<u8> {
    for jump in jumps {
        let offset = jump.push_offset;
        let size = jump.push_opcode - PUSH0_OPCODE as usize + 1;

        let offset = offset + shifts[offset];
        let dst = jump.dst + shifts[jump.dst];
        let mut instruction = vec![PUSH32_OPCODE];
        instruction.extend(bytes32_from_usize(dst));
        prelude.splice(offset..offset + size, instruction);
    }

    prelude
}

fn filled_bytes(mut n: usize) -> usize {
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

fn usize_from_bytes(bytes: &[u8]) -> usize {
    let size = bytes.len();
    assert!(size < USIZE_BYTES);
    let mut b = [0; USIZE_BYTES];
    b[USIZE_BYTES - size..].copy_from_slice(bytes);
    usize::from_be_bytes(b)
}

#[cfg(test)]
mod tests {
    use super::{
        amend, create_push_instruction, get_jumps, usize_from_bytes, Binary, Jump, PUSH0_OPCODE,
    };

    #[test]
    fn creates_usize_from_be_bytes() {
        assert_eq!(0, usize_from_bytes(&[0]));
        assert_eq!(1, usize_from_bytes(&[1]));
        assert_eq!(256, usize_from_bytes(&[1, 0]));
    }

    #[test]
    fn amends_ctr_no_arguments() {
        let prelude =
            hex::decode("6080604052348015600e575f80fd5b5060055f55603e80601e5f395ff3fe").unwrap();
        let runtime = hex::decode("60806040525f80fdfea26469706673582212201930c24a9bacd514d80b227d7f262dc1678997d986b05b354f9d092c2fcaee5864736f6c63430008150033").unwrap();
        let binary = Binary {
            prelude,
            code: runtime,
        };

        let binary = amend(binary, 3388).unwrap();
        let expected = hex::decode("60806040523480157f000000000000000000000000000000000000000000000000000000000000002d575f80fd5b5060055f557f0000000000000000000000000000000000000000000000000000000000000d3c807f000000000000000000000000000000000000000000000000000000000000007b5f395ff3fe").unwrap();
        assert_eq!(hex::encode(expected), hex::encode(binary.prelude));
    }

    #[test]
    fn creates_push_instruction() {
        // Ideally, this test uses fuzzing.
        let push_0 = create_push_instruction(0);
        assert_eq!(push_0, vec![PUSH0_OPCODE]);

        let push_1 = create_push_instruction(1);
        assert_eq!(push_1, vec![PUSH0_OPCODE + 1, 0x1]);

        let max_value = 0xFFFFFFFFFFFFFFFF;
        let push_8 = create_push_instruction(max_value);
        let mut actual = vec![PUSH0_OPCODE + 8];
        actual.extend(max_value.to_be_bytes());
        assert_eq!(push_8, actual);

        let value = 0x0F00000000;
        let push_5 = create_push_instruction(value);
        let mut actual = vec![PUSH0_OPCODE + 5];
        actual.extend(hex::decode("0F00000000").unwrap());
        assert_eq!(push_5, actual);
    }

    #[test]
    fn finds_jumps() {
        let bytecode =
            hex::decode("6080604052348015600e575f80fd5b5060055f55603e80601e5f395ff3fe").unwrap();
        let jumps = get_jumps(&bytecode);
        let expected = Jump {
            push_offset: 8,
            push_opcode: 0x60,
            dst: 0x0e,
        };
        assert_eq!(vec![expected], jumps);

        let bytecode =
            hex::decode("6080604052348015600e575f80fd5b5060055f5561010e56603e80601e5f395ff3fe")
                .unwrap();
        let jumps = get_jumps(&bytecode);
        let expected = vec![
            Jump {
                push_offset: 8,
                push_opcode: 0x60,
                dst: 0x0e,
            },
            Jump {
                push_offset: 20,
                push_opcode: 0x61,
                dst: 0x010e,
            },
        ];
        assert_eq!(expected, jumps);

        let bytecode =
            hex::decode("6080604052348015600e575f80fd5b50604051608e380380608e833981016040819052602991602f565b5f556045565b5f60208284031215603e575f80fd5b5051919050565b603e8060505f395ff3fe")
                .unwrap();
        let jumps = get_jumps(&bytecode);
        let expected = vec![
            Jump {
                push_offset: 8,
                push_opcode: 96,
                dst: 14,
            },
            Jump {
                push_offset: 38,
                push_opcode: 96,
                dst: 47,
            },
            Jump {
                push_offset: 44,
                push_opcode: 96,
                dst: 69,
            },
            Jump {
                push_offset: 56,
                push_opcode: 96,
                dst: 62,
            },
        ];
        assert_eq!(expected, jumps);
    }
}
