use std::mem;

use eyre::bail;
use once_cell::sync::Lazy;
use regex::{Regex, RegexBuilder};

use super::{instruction, opcode};

#[derive(Debug, Clone, PartialEq)]
pub struct Opcode {
    pub name: String,
    pub hex: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Operator {
    pub name: String,
    pub arg: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    // 0xfe -> invalid
    Opcode(Opcode),
    // 0x12345678
    Constant(String),
    // `dataSize` or `dataOffset`
    Operator(Operator),
    // `bytecodeSize`
    Builtin(String),
    LabelBegin(String),
    LabelEnd,
}

impl Token {
    pub fn opcode(byte: u8) -> Self {
        Token::Opcode(Opcode {
            name: instruction(byte).unwrap(),
            hex: hex::encode([byte]),
        })
    }

    pub fn constant(constant: &str) -> Self {
        Token::Constant(constant.to_owned())
    }

    /// Size in bytes of this token bytecode representation.
    pub fn size(&self) -> usize {
        match self {
            Token::Opcode(_) => 1,
            Token::Constant(c) => c.len() / 2,
            Token::Builtin(_) => 33, // PUSH + 32 bytes.
            Token::Operator(_) | Token::LabelBegin(_) | Token::LabelEnd => 0,
        }
    }

    pub fn bytecode(&self) -> eyre::Result<Vec<u8>> {
        match self {
            Token::Opcode(op) => hex::decode(&op.hex).map_err(|e| e.into()),
            Token::Constant(c) => hex::decode(c).map_err(|e| e.into()),
            Token::Operator(_) | Token::LabelBegin(_) | Token::LabelEnd | Token::Builtin(_) => {
                bail!("unexpected token found when generating bytecode")
            }
        }
    }
}

pub fn tokenize(instructions: Vec<String>) -> Vec<Token> {
    instructions
        .into_iter()
        .flat_map(|s| tokenize_part(&s))
        .collect()
}

fn tokenize_part(instruction: &str) -> Vec<Token> {
    if let Some(byte) = opcode(instruction) {
        return vec![Token::opcode(byte)];
    }

    if let Some(tokens) = tokenize_constant(instruction) {
        return tokens;
    }

    if let Some(tokens) = tokenize_auxdata(instruction) {
        return tokens;
    }

    if let Some(tokens) = tokenize_builtin(instruction) {
        return tokens;
    }

    if let Some(tokens) = tokenize_operator(instruction) {
        return tokens;
    }

    if let Some(tokens) = tokenize_call(instruction) {
        return tokens;
    }

    if let Some(tokens) = tokenize_label(instruction) {
        return tokens;
    }

    vec![Token::Operator(Operator {
        name: "dataOffset".to_owned(),
        arg: instruction.to_owned(),
    })]
}

fn tokenize_constant(instruction: &str) -> Option<Vec<Token>> {
    static HEX_LITERAL: Lazy<Regex> = Lazy::new(|| {
        RegexBuilder::new(r"^0x[\da-f]+$")
            .case_insensitive(true)
            .build()
            .unwrap()
    });

    if HEX_LITERAL.is_match(instruction) {
        return Some(push_constant(&instruction[2..]));
    }

    None
}

fn prepend_zeros(to: &str) -> String {
    if to.len() % 2 == 1 {
        return format!("0{to}");
    }

    to.to_owned()
}

pub fn push_constant(constant: &str) -> Vec<Token> {
    let constant = prepend_zeros(constant);

    // Special case `PUSH0`.
    let constant_is_zero = hex::decode(&constant).unwrap().iter().all(|b| *b == 0);
    if constant_is_zero {
        return vec![Token::opcode(opcode("PUSH0").unwrap())];
    }

    let size = constant.len() / 2;
    let push = format!("PUSH{size}");
    let op = opcode(&push).expect("constant size should be less than 32 bytes");
    vec![Token::opcode(op), Token::constant(&constant)]
}

fn tokenize_auxdata(instruction: &str) -> Option<Vec<Token>> {
    let prefix = "auxdata:";
    if !instruction.starts_with(prefix) {
        return None;
    }

    let data = instruction.chars().skip(prefix.len()).collect::<String>();
    Some(vec![Token::constant(&data[2..])])
}

fn tokenize_operator(instruction: &str) -> Option<Vec<Token>> {
    static OPERATORS: Lazy<Regex> = Lazy::new(|| {
        RegexBuilder::new(r"^(dataSize|dataOffset)\((.+)\)$")
            .case_insensitive(true)
            .build()
            .unwrap()
    });

    if let Some(captures) = OPERATORS.captures(instruction) {
        return Some(vec![Token::Operator(Operator {
            name: captures[1].to_owned(),
            arg: captures[2].to_owned(),
        })]);
    }

    None
}

fn tokenize_call(instruction: &str) -> Option<Vec<Token>> {
    static FUNCTION_CALL: Lazy<Regex> = Lazy::new(|| Regex::new(r"^([^\(\)]+)\((.*)\)$").unwrap());

    if let Some(captures) = FUNCTION_CALL.captures(instruction) {
        let f = tokenize_part(&captures[1]);
        let args = tokenize_args(&captures[2]);

        return Some([args, f].concat());
    }

    None
}

fn tokenize_args(args: &str) -> Vec<Token> {
    // Simple case: no nested arguments.
    if !args.contains('(') {
        static COMMA_SPACE: Lazy<Regex> = Lazy::new(|| Regex::new(r",\s*").unwrap());
        let args: Vec<&str> = COMMA_SPACE.split(args).collect();
        return args.into_iter().rev().flat_map(tokenize_part).collect();
    }

    let mut tokens = vec![];

    let args = args.chars().collect::<Vec<char>>();

    let mut i = 0;
    let mut current = String::new();
    while i < args.len() {
        match args[i] {
            ',' => {
                if !current.is_empty() {
                    tokens.extend(tokenize_part(&mem::take(&mut current)));
                }
                i += 1;
            }
            '(' => {
                let opcode = tokenize_part(&mem::take(&mut current));
                let mut inner = vec![];
                i += 1;
                let mut parens = 1;
                loop {
                    match args[i] {
                        '(' => parens += 1,
                        ')' => parens -= 1,
                        _ => {}
                    }
                    if args[i] == ')' && parens == 0 {
                        i += 1;
                        break;
                    }
                    inner.push(args[i]);
                    i += 1;
                }

                let inner: String = inner[..inner.len()].into_iter().collect();
                tokens.extend(tokenize_args(&inner));
                tokens.extend(opcode);
                current.clear();
            }
            c => {
                current.push(c);
                i += 1;
            }
        }
    }

    if !current.is_empty() {
        let mut t = tokenize_part(&current);
        t.extend(tokens);
        tokens = t;
    }

    tokens
}

fn tokenize_label(instruction: &str) -> Option<Vec<Token>> {
    static SINGLE_LINE_LABEL: Lazy<Regex> = Lazy::new(|| {
        RegexBuilder::new(r"^([a-z][a-z\d_]*):$")
            .case_insensitive(true)
            .build()
            .unwrap()
    });
    static MULTI_LINE_LABEL: Lazy<Regex> = Lazy::new(|| {
        RegexBuilder::new(r"^([a-z][a-z\d_]*):\s*assembly\s*\{$")
            .case_insensitive(true)
            .build()
            .unwrap()
    });

    if let Some(captures) = SINGLE_LINE_LABEL.captures(instruction) {
        return Some(vec![
            Token::LabelBegin(captures[1].to_owned()),
            Token::LabelEnd,
            Token::opcode(opcode("JUMPDEST").unwrap()),
        ]);
    }

    if let Some(captures) = MULTI_LINE_LABEL.captures(instruction) {
        return Some(vec![Token::LabelBegin(captures[1].to_owned())]);
    }

    if instruction == "}" {
        return Some(vec![Token::LabelEnd]);
    }

    None
}

fn tokenize_builtin(instruction: &str) -> Option<Vec<Token>> {
    static BUILTINS: Lazy<Regex> = Lazy::new(|| {
        RegexBuilder::new(r"^(bytecodeSize)$")
            .case_insensitive(true)
            .build()
            .unwrap()
    });

    if let Some(captures) = BUILTINS.captures(instruction) {
        return Some(vec![Token::Builtin(captures[1].to_owned())]);
    }

    None
}

pub fn amend(evmasm: &str, wasm: &[u8]) -> String {
    static AUXDATA_BLOCK: Lazy<Regex> =
        Lazy::new(|| Regex::new(r"([\S\s]*\n.*:.*assembly.*)\{[\S\s]*auxdata:[\S\s]*\}").unwrap());

    let wasm = format!("0x{}", hex::encode(wasm));
    let runtime = format!(
        "$1 {{
auxdata: {wasm}
}}"
    );
    let asm = AUXDATA_BLOCK.replace(evmasm, runtime);
    asm.to_string()
}

pub fn clean_asm(evmasm: &str) -> Vec<String> {
    let asm = remove_comments(evmasm);
    let asm = remove_space_around_symbols(&asm);
    let asm = reduce_spaces(&asm);

    let instructions = asm
        .split(' ')
        .filter(|s| !s.is_empty())
        .map(|s| s.to_owned())
        .collect();
    instructions
}

fn remove_comments(asm: &str) -> String {
    static MULTI_LINE_COMMENT: Lazy<Regex> = Lazy::new(|| Regex::new(r"\/\*.*\*\/").unwrap());
    static SINGLE_LINE_COMMENT: Lazy<Regex> = Lazy::new(|| Regex::new(r"\/\/.*").unwrap());
    let asm = MULTI_LINE_COMMENT.replace_all(asm, "");
    let asm = SINGLE_LINE_COMMENT.replace_all(&asm, "");
    asm.to_string()
}

fn reduce_spaces(asm: &str) -> String {
    static SPACES: Lazy<Regex> = Lazy::new(|| Regex::new(r"(\s|\n)+").unwrap());
    let asm = SPACES.replace_all(asm, " ");
    asm.to_string()
}

fn remove_space_around_symbols(asm: &str) -> String {
    // Matches spaces surrounding the `(,:{` characters.
    static SPACE_AROUND_SYMBOLS: Lazy<Regex> =
        Lazy::new(|| Regex::new(r"[^\S\r\n]*([(,:{])[^\S\r\n]*").unwrap());
    let asm = SPACE_AROUND_SYMBOLS.replace_all(asm, "$1");

    static SPACE_BEFORE_PAREN_BRACE: Lazy<Regex> =
        Lazy::new(|| Regex::new(r"[^\S\r\n]*(\)})").unwrap());
    let asm = SPACE_BEFORE_PAREN_BRACE.replace_all(&asm, "$1");
    asm.to_string()
}

#[cfg(test)]
mod tests {
    use crate::assembler::{opcode, tokenizer::Operator};

    use super::{push_constant, reduce_spaces, remove_comments, tokenize, Token};

    #[test]
    fn removes_comments() {
        let asm = r##"jump// out
/* "#utility.yul":694:1045   */
tag_3:"##;
        let actual = remove_comments(asm);
        let expected = "jump\n\ntag_3:";
        assert_eq!(expected, actual);
    }

    #[test]
    fn reduces_spaces() {
        let asm = "\
  dataSize(sub_0)
  dup1
  dataOffset(sub_0)
  0x00
  codecopy
  0x00
  return
  stop";
        let actual = reduce_spaces(asm);
        let expected = "dataSize(sub_0) dup1 dataOffset(sub_0) 0x00 codecopy 0x00 return stop";
        assert_eq!(expected, actual);
    }

    #[test]
    fn calls() {
        let actual = tokenize(vec!["mstore(0x40,0x80)".to_owned()]);
        let mut expected = vec![];
        expected.extend(push_constant("80"));
        expected.extend(push_constant("40"));
        expected.push(Token::opcode(opcode("mstore").unwrap()));
        assert_eq!(expected, actual);

        let actual = tokenize(vec!["calldatacopy(0x1,0x2,calldatasize)".to_owned()]);
        let mut expected = vec![];
        expected.push(Token::opcode(opcode("calldatasize").unwrap()));
        expected.extend(push_constant("02"));
        expected.extend(push_constant("01"));
        expected.push(Token::opcode(opcode("calldatacopy").unwrap()));
        assert_eq!(expected, actual);
    }

    #[test]
    fn tokenizes_opcodes() {
        let stream = "push1 push1 mstore"
            .split_whitespace()
            .map(|t| t.to_owned())
            .collect();
        let actual = tokenize(stream);
        let mut expected = vec![];
        expected.push(Token::opcode(opcode("push1").unwrap()));
        expected.push(Token::opcode(opcode("push1").unwrap()));
        expected.push(Token::opcode(opcode("mstore").unwrap()));
        assert_eq!(expected, actual);
    }

    #[test]
    fn tokenizes_constants() {
        let stream = "0x80 0x40 mstore"
            .split_whitespace()
            .map(|t| t.to_owned())
            .collect();
        let actual = tokenize(stream);
        let mut expected = vec![];
        expected.extend(push_constant("80"));
        expected.extend(push_constant("40"));
        expected.push(Token::opcode(opcode("mstore").unwrap()));
        assert_eq!(expected, actual);

        let stream = "0x1e4fbdf700000000000000000000000000000000000000000000000000000000"
            .split_whitespace()
            .map(|t| t.to_owned())
            .collect();
        let actual = tokenize(stream);
        let mut expected = vec![];
        expected.extend(push_constant(
            "1e4fbdf700000000000000000000000000000000000000000000000000000000",
        ));
        assert_eq!(expected, actual);
    }

    #[test]
    fn tokenizes_labels() {
        let stream = "tag_1: pop"
            .split_whitespace()
            .map(|t| t.to_owned())
            .collect();
        let actual = tokenize(stream);
        let mut expected = vec![];
        expected.push(Token::LabelBegin("tag_1".to_owned()));
        expected.push(Token::LabelEnd);
        expected.push(Token::opcode(opcode("jumpdest").unwrap()));
        expected.push(Token::opcode(opcode("pop").unwrap()));
        assert_eq!(expected, actual);
    }

    #[test]
    fn tokenizes_auxdata() {
        let stream = "auxdata:0x1234"
            .split_whitespace()
            .map(|t| t.to_owned())
            .collect();
        let actual = tokenize(stream);
        let mut expected = vec![];
        expected.push(Token::constant("1234"));
        assert_eq!(expected, actual);
    }

    #[test]
    fn tokenizes_assembly_block() {
        let stream = "sub_0:assembly{ dup1 }"
            .split_whitespace()
            .map(|t| t.to_owned())
            .collect();
        let actual = tokenize(stream);
        let mut expected = vec![];
        expected.push(Token::LabelBegin("sub_0".to_owned()));
        expected.push(Token::opcode(opcode("dup1").unwrap()));
        expected.push(Token::LabelEnd);
        assert_eq!(expected, actual);
    }

    #[test]
    fn tokenizes_label_references() {
        let stream = "dup1 label_0 dup2 label_0: dup3 dup4"
            .split_whitespace()
            .map(|t| t.to_owned())
            .collect();
        let actual = tokenize(stream);
        let mut expected = vec![];
        expected.push(Token::opcode(opcode("dup1").unwrap()));
        expected.push(Token::Operator(Operator {
            name: "dataOffset".to_owned(),
            arg: "label_0".to_owned(),
        }));
        expected.push(Token::opcode(opcode("dup2").unwrap()));
        expected.push(Token::LabelBegin("label_0".to_owned()));
        expected.push(Token::LabelEnd);
        expected.push(Token::opcode(opcode("jumpdest").unwrap()));
        expected.push(Token::opcode(opcode("dup3").unwrap()));
        expected.push(Token::opcode(opcode("dup4").unwrap()));
        assert_eq!(expected, actual);
    }

    #[test]
    fn tokenizes_nested_args() {
        let stream = "sub(shl(0xa0,0x02),0x01)"
            .split_whitespace()
            .map(|t| t.to_owned())
            .collect();
        let actual = tokenize(stream);
        let mut expected = vec![];
        expected.extend(push_constant("01"));
        expected.extend(push_constant("02"));
        expected.extend(push_constant("a0"));
        expected.push(Token::opcode(opcode("shl").unwrap()));
        expected.push(Token::opcode(opcode("sub").unwrap()));
        assert_eq!(expected, actual);

        let stream = "sub(codecopy(0xa0,0x02,0x03),0x01)"
            .split_whitespace()
            .map(|t| t.to_owned())
            .collect();
        let actual = tokenize(stream);
        let mut expected = vec![];
        expected.extend(push_constant("01"));
        expected.extend(push_constant("03"));
        expected.extend(push_constant("02"));
        expected.extend(push_constant("a0"));
        expected.push(Token::opcode(opcode("codecopy").unwrap()));
        expected.push(Token::opcode(opcode("sub").unwrap()));
        assert_eq!(expected, actual);

        let stream = "not(sub(shl(0xa0,0x02),0x01))"
            .split_whitespace()
            .map(|t| t.to_owned())
            .collect();
        let actual = tokenize(stream);
        let mut expected = vec![];
        expected.extend(push_constant("01"));
        expected.extend(push_constant("02"));
        expected.extend(push_constant("a0"));
        expected.push(Token::opcode(opcode("shl").unwrap()));
        expected.push(Token::opcode(opcode("sub").unwrap()));
        expected.push(Token::opcode(opcode("not").unwrap()));
        assert_eq!(expected, actual);
    }
}
