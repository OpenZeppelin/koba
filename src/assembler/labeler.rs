use std::collections::HashMap;

use crate::assembler::tokenizer::push_constant;

use super::tokenizer::Token;

struct Label {
    name: String,
    index: usize,
    size: usize,
}

pub fn labelize(stream: &[Token]) -> Vec<Token> {
    let mut labels = HashMap::new();
    let mut stack = Vec::new();

    let mut index = 0;
    let label_size = estimate_max_label_size(stream);
    for token in stream {
        match token {
            Token::LabelBegin(name) => {
                stack.push(Label {
                    name: name.clone(),
                    index,
                    size: 0,
                });
                index = 0;
            }
            Token::LabelEnd => {
                let label = stack.pop();
                let Some(label) = label else {
                    // TODO: Maybe make this fallible?
                    panic!("Imbalanced labels at index {index}");
                };
                index += label.index;
                labels.insert(
                    label.name.clone(),
                    Label {
                        name: label.name,
                        index: label.index,
                        size: index,
                    },
                );
            }
            Token::Opcode(_) | Token::Constant(_) => index += token.size(),
            Token::Operator(operator) if operator.name == "dataOffset" => {
                index += 1; // A PUSH instruction.
                index += label_size;
            }
            Token::Operator(_) => {
                index += 1; // A PUSH instruction.
                index += 32; // We can't know datasize here.
            }
            Token::Builtin(_) => {
                index += 1; // A PUSH instruction.
                index += token.size() - 1
            }
        }
    }

    let bytecode = stream
        .iter()
        .filter(|t| !matches!(t, Token::LabelBegin(_) | Token::LabelEnd))
        .flat_map(|t| match t {
            Token::Operator(operator) => {
                // TODO: Maybe make this fallible?
                let label = labels
                    .get(&operator.arg)
                    .unwrap_or_else(|| panic!("Label '{}' not found", operator.arg));

                let tokens = match operator.name.as_ref() {
                    "dataOffset" => {
                        let constant = &format!("{:0width$x}", label.index, width = label_size * 2);
                        push_constant(constant)
                    }
                    "dataSize" => {
                        let constant = &format!("{:0width$x}", label.size, width = 64);
                        push_constant(constant)
                    }
                    _ => unreachable!(),
                };

                tokens
            }
            Token::Builtin(_) => vec![t.clone()],
            Token::Opcode(_) | Token::Constant(_) => vec![t.clone()],
            Token::LabelBegin(_) | Token::LabelEnd => unreachable!(),
        })
        .collect::<Vec<_>>();

    let bytecode_size: usize = stream
        .iter()
        .map(|t| match t {
            Token::Opcode(_) | Token::Constant(_) | Token::Builtin(_) => t.size(),
            Token::Operator(operator) => {
                // TODO: Maybe make this fallible?
                let label = labels
                    .get(&operator.arg)
                    .unwrap_or_else(|| panic!("Label '{}' not found", operator.arg));

                let size = match operator.name.as_ref() {
                    "dataOffset" => {
                        let constant = &format!("{:0width$x}", label.index, width = label_size * 2);
                        1 + constant.len() / 2
                    }
                    "dataSize" => {
                        let constant = &format!("{:0width$x}", label.size, width = 64);
                        1 + constant.len() / 2
                    }
                    _ => unreachable!(),
                };

                size
            }
            Token::LabelBegin(_) | Token::LabelEnd => 0,
        })
        .sum();

    bytecode
        .into_iter()
        .flat_map(|t| match t {
            Token::LabelBegin(_) | Token::LabelEnd => unreachable!(),
            // TODO: Compute size properly instead of using 32 bytes.
            Token::Builtin(_) => {
                let constant = &format!("{:0width$x}", bytecode_size, width = 64);
                push_constant(constant)
            }
            Token::Opcode(_) | Token::Constant(_) | Token::Operator(_) => vec![t],
        })
        .collect()
}

/// Estimates the maximum label size.
///
/// That is, how many hex digits do we need to represent all the addressable
/// contract offsets, including labels.
fn estimate_max_label_size(stream: &[Token]) -> usize {
    let contract_size_without_labels: usize = stream
        .iter()
        .take(stream.len() - 2) // Skip runtime.
        .map(|t| match t {
            // We patch here to compute roughly correct label sizes while
            // defending against the worst case: it can't happen that we
            // undercount because then jump offsets won't be computed correctly.
            Token::Operator(_) => 33,
            _ => t.size(),
        })
        .sum();
    let label_count = stream
        .iter()
        .filter(|t| matches!(t, Token::Operator(_)))
        .count();
    // Tbh, impossible to reach...
    let max_label_size: usize = 64;
    // We are looking for the number of hex digits such that the contract size
    // "fits" in.
    let mut hex_digits = 2;
    while hex_digits < max_label_size {
        let contract_size: usize =
            contract_size_without_labels + (1 + hex_digits / 2) * label_count;

        if 16_usize.pow(hex_digits as u32) >= contract_size {
            return hex_digits / 2;
        }

        hex_digits += 2;
    }

    max_label_size / 2
}

#[cfg(test)]
mod tests {
    use crate::assembler::tokenizer;

    use super::estimate_max_label_size;

    #[test]
    fn estimates_max_label_size() {
        let asm = r##"
  mstore(0x40, 0x80)
  callvalue
  dup1
  iszero
  tag_1
  jumpi
  0x00
  dup1
  revert
  tag_1:
  pop
  mload(0x40)
  sub(codesize, bytecodeSize)
  dup1
  bytecodeSize
  dup4
  codecopy
  dup2
  add
  0x40
  dup2
  swap1
  mstore
  tag_2
  swap2
  tag_3
  jump
  tag_2:
  0x00
  sstore
  jump(tag_7)
  tag_3:
  0x00
  0x20
  dup3
  dup5
  sub
  slt
  iszero
  tag_9
  jumpi
  0x00
  dup1
  revert
  tag_9:
  pop
  mload
  swap2
  swap1
  pop
  jump
  tag_7:
  dataSize(sub_0)
  dup1
  dataOffset(sub_0)
  0x00
  codecopy
  0x00
  return
  stop
  
sub_0: assembly {
  auxdata: 0x00
}"##;

        let instructions = tokenizer::clean_asm(asm);
        let stream = tokenizer::tokenize(instructions);
        let max_size = estimate_max_label_size(&stream);
        assert_eq!(max_size, 2);

        let asm = r##"
  mstore(0x40, 0x80)
  callvalue
  dup1
  iszero
  tag_1
  jumpi
  0x00
  dup1
  revert
tag_1:
  pop
  mload(0x40)
  sub(codesize, 0x00)
  dup1
  0x00
  dup4
  codecopy
  dup2
  add
  0x40
  dup2
  swap1
  mstore
  tag_2
  swap2
  tag_3
  jump	// in
tag_2:
  sub(shl(0xa0, 0x01), 0x01)
  dup2
  and
  tag_6
  jumpi
  mload(0x40)
  shl(0xe0, 0x1e4fbdf7)
  dup2
  mstore
  0x00
  0x04
  dup3
  add
  mstore
  0x24
  add
  mload(0x40)
  dup1
  swap2
  sub
  swap1
  revert
tag_6:
  0x00
  dup1
  sload
  sub(shl(0xa0, 0x01), 0x01)
  dup4
  dup2
  and
  not(sub(shl(0xa0, 0x01), 0x01))
  dup4
  and
  dup2
  or
  dup5
  sstore
  mload(0x40)
  swap2
  swap1
  swap3
  and
  swap3
  dup4
  swap2
  0x8be0079c531659141344cd1fd0a4f28419497f9722a3daafe3b4186f6b6457e0
  swap2
  swap1
  log3
  pop
  pop
  jump(tag_10)
tag_3:
  0x00
  0x20
  dup3
  dup5
  sub
  slt
  iszero
  tag_12
  jumpi
  0x00
  dup1
  revert
tag_12:
  dup2
  mload
  sub(shl(0xa0, 0x01), 0x01)
  dup2
  and
  dup2
  eq
  tag_13
  jumpi
  0x00
  dup1
  revert
tag_13:
  swap4
  swap3
  pop
  pop
  pop
  jump	// out
tag_10:
  dataSize(sub_0)
  dup1
  dataOffset(sub_0)
  0x00
  codecopy
  0x00
  return
stop
sub_0: assembly {
  auxdata: 0x00000000000000000000
}"##;

        let instructions = tokenizer::clean_asm(asm);
        let stream = tokenizer::tokenize(instructions);
        let max_size = estimate_max_label_size(&stream);
        assert_eq!(max_size, 2);
    }
}
