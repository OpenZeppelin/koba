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
                        let label_size = label_size + label_size % 2;
                        let constant = &format!("{:0width$x}", label.index, width = label_size);
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
                        let label_size = label_size + label_size % 2;
                        let constant = &format!("{:0width$x}", label.index, width = label_size);
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
    let contract_size_without_labels: usize = stream.iter().map(|t| t.size()).sum();
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

        hex_digits += 1;
    }

    max_label_size / 2
}

#[cfg(test)]
mod tests {
    use crate::assembler::tokenizer::Token;
    use crate::assembler::{opcode, tokenizer::Operator};

    use super::estimate_max_label_size;

    macro_rules! op {
        ($op: literal) => {
            Token::opcode(opcode($op).unwrap())
        };
    }

    macro_rules! constant {
        ($c: literal) => {
            Token::Constant($c.to_owned())
        };
    }

    macro_rules! label_begin {
        ($c: literal) => {
            Token::LabelBegin($c.to_owned())
        };
    }

    #[test]
    fn estimates_max_label_size() {
        let stream = [
            op!("push1"),
            constant!("80"),
            op!("push1"),
            constant!("40"),
            op!("mstore"),
            op!("callvalue"),
            op!("dup1"),
            op!("iszero"),
            Token::Operator(Operator {
                name: "dataOffset".to_owned(),
                arg: "tag_1".to_owned(),
            }),
            op!("jumpi"),
            op!("push0"),
            op!("dup1"),
            op!("revert"),
            label_begin!("tag_1"),
            Token::LabelEnd,
            op!("jumpdest"),
            op!("pop"),
            op!("push1"),
            constant!("40"),
            op!("mload"),
            Token::Builtin("bytecodeSize".to_owned()),
            op!("codesize"),
            op!("sub"),
            op!("dup1"),
            Token::Builtin("bytecodeSize".to_owned()),
            op!("dup4"),
            op!("codecopy"),
            op!("dup2"),
            op!("add"),
            op!("push1"),
            constant!("40"),
            op!("dup2"),
            op!("swap1"),
            op!("mstore"),
            Token::Operator(Operator {
                name: "dataOffset".to_owned(),
                arg: "tag_2".to_owned(),
            }),
            op!("swap2"),
            Token::Operator(Operator {
                name: "dataOffset".to_owned(),
                arg: "tag_3".to_owned(),
            }),
            op!("jump"),
            label_begin!("tag_2"),
            Token::LabelEnd,
            op!("jumpdest"),
            op!("push0"),
            op!("sstore"),
            Token::Operator(Operator {
                name: "dataOffset".to_owned(),
                arg: "tag_7".to_owned(),
            }),
            op!("jump"),
            label_begin!("tag_3"),
            Token::LabelEnd,
            op!("jumpdest"),
            op!("push0"),
            op!("push1"),
            constant!("20"),
            op!("dup3"),
            op!("dup5"),
            op!("sub"),
            op!("slt"),
            op!("iszero"),
            Token::Operator(Operator {
                name: "dataOffset".to_owned(),
                arg: "tag_9".to_owned(),
            }),
            op!("jumpi"),
            op!("push0"),
            op!("dup1"),
            op!("revert"),
            label_begin!("tag_9"),
            Token::LabelEnd,
            op!("jumpdest"),
            op!("pop"),
            op!("mload"),
            op!("swap2"),
            op!("swap1"),
            op!("pop"),
            op!("jump"),
            label_begin!("tag_7"),
            Token::LabelEnd,
            op!("jumpdest"),
            Token::Operator(Operator {
                name: "dataSize".to_owned(),
                arg: "sub_0".to_owned(),
            }),
            op!("dup1"),
            Token::Operator(Operator {
                name: "dataOffset".to_owned(),
                arg: "sub_0".to_owned(),
            }),
            op!("push0"),
            op!("codecopy"),
            op!("push0"),
            op!("return"),
            op!("stop"),
            label_begin!("sub_0"),
            constant!("eff00000"),
            Token::LabelEnd,
        ];

        let max_size = estimate_max_label_size(&stream);
        assert_eq!(max_size, 1);
    }
}
