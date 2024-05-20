mod instruction;
mod labelizer;
mod tokenizer;

pub use instruction::{instruction, opcode};

pub fn compile(evmasm: &str, wasm: &[u8]) -> eyre::Result<String> {
    let evmasm = tokenizer::amend(evmasm, wasm);
    let instructions = tokenizer::clean_asm(&evmasm);
    let stream = tokenizer::tokenize(instructions);
    let stream = labelizer::labelize(&stream);

    let mut bytecode = Vec::with_capacity(stream.len() * 2);
    for token in stream {
        let bytes = token.bytecode()?;
        bytecode.extend(bytes);
    }

    Ok(hex::encode(&bytecode))
}

#[cfg(test)]
mod tests {
    use super::{
        labelizer, opcode,
        tokenizer::{self, push_constant, Token},
    };

    fn to_hex_code(evmasm: &str) -> Vec<Token> {
        let instructions = tokenizer::clean_asm(&evmasm);
        let stream = tokenizer::tokenize(instructions);
        let stream = labelizer::labelize(&stream);
        stream
    }

    macro_rules! op {
        ($op: literal) => {
            Token::opcode(opcode($op).unwrap())
        };
    }

    #[test]
    fn converts_labels() {
        let actual = to_hex_code(
            "
dup1
label_0
dup1
label_0:
dup1
dup1",
        );

        let mut expected = vec![];
        expected.push(op!("DUP1"));
        expected.extend(push_constant("04"));
        expected.push(op!("DUP1"));
        expected.push(op!("JUMPDEST"));
        expected.push(op!("DUP1"));
        expected.push(op!("DUP1"));

        assert_eq!(expected, actual);
    }

    #[test]
    fn converts_data_size() {
        let actual = to_hex_code(
            "
dup1
dataSize(label_0)
dup2
label_0: assembly {
    dup3
    label_1:
    dup4
    label_1
}
dup5
",
        );

        let mut expected = vec![];
        expected.push(op!("DUP1"));
        expected.extend(push_constant("05"));
        expected.push(op!("DUP2"));
        expected.push(op!("DUP3"));
        expected.push(op!("JUMPDEST"));
        expected.push(op!("DUP4"));
        expected.extend(push_constant("01"));
        expected.push(op!("DUP5"));

        assert_eq!(expected, actual);
    }
}
