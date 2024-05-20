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
        println!("{:x}: {:?}", index, token);
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
            Token::Operator(_) => {
                index += 1; // A PUSH instruction.
                index += label_size;
            }
            Token::Builtin(_) => {
                index += 1; // A PUSH instruction.
                index += token.size()
            }
        }
    }

    let stream = stream
        .iter()
        .filter(|t| !matches!(t, Token::LabelBegin(_) | Token::LabelEnd))
        .map(|t| match t {
            Token::Operator(operator) => {
                // TODO: Maybe make this fallible?
                let label = labels
                    .get(&operator.arg)
                    .expect(&format!("Label '{}' not found", operator.arg));

                let tokens = match operator.name.as_ref() {
                    "dataOffset" => {
                        let label_size = label_size + label_size % 2;
                        let constant = &format!("{:0width$x}", label.index, width = label_size);
                        println!("{}: {} | {}", operator.arg, constant, label_size);
                        push_constant(&constant)
                    }
                    "dataSize" => {
                        let label_size = label_size + label_size % 2;
                        let constant = &format!("{:0width$x}", label.size, width = label_size);
                        println!(
                            "{}: {} | {} | {}",
                            operator.arg, constant, label_size, label.size
                        );
                        push_constant(&constant)
                    }
                    _ => unreachable!(),
                };

                tokens
            }
            Token::Builtin(_) => vec![t.clone()],
            Token::Opcode(_) | Token::Constant(_) => vec![t.clone()],
            Token::LabelBegin(_) | Token::LabelEnd => unreachable!(),
        })
        .flatten()
        .collect::<Vec<_>>();
    println!("{:?}", stream);

    let bytecode_size: usize = stream.iter().map(|t| t.size()).sum();

    stream
        .into_iter()
        .map(|t| match t {
            Token::LabelBegin(_) | Token::LabelEnd => unreachable!(),
            // TODO: Compute size properly instead of using 32 bytes.
            Token::Builtin(_) => {
                let constant = &format!("{:0width$x}", bytecode_size, width = 64);
                push_constant(constant)
            }
            Token::Opcode(_) | Token::Constant(_) | Token::Operator(_) => vec![t],
        })
        .flatten()
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
    let mut hex_digits = 1;
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
