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

    pub fn constant(constant: String) -> Self {
        Token::Constant(constant)
    }

    /// Size in bytes of this token bytecode representation.
    pub fn size(&self) -> usize {
        match self {
            Token::Opcode(_) => 1,
            Token::Constant(c) => c.len() / 2,
            Token::Builtin(_) => 33, // PUSH + 32 bytes,
            Token::Operator(_) | Token::LabelBegin(_) | Token::LabelEnd => 0,
        }
    }

    pub fn bytecode(&self) -> eyre::Result<Vec<u8>> {
        match self {
            Token::Opcode(op) => hex::decode(&op.hex).map_err(|e| e.into()),
            Token::Constant(c) => hex::decode(&c).map_err(|e| e.into()),
            Token::Operator(_) | Token::LabelBegin(_) | Token::LabelEnd | Token::Builtin(_) => {
                bail!("unexpected token found when generating bytecode")
            }
        }
    }
}

pub fn tokenize(instructions: Vec<String>) -> Vec<Token> {
    instructions
        .into_iter()
        .map(|s| tokenize_part(&s))
        .flatten()
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
    let op = opcode(&push).expect("constant size is bigger than 32 bytes");
    vec![Token::opcode(op), Token::constant(constant.to_owned())]
}

fn tokenize_auxdata(instruction: &str) -> Option<Vec<Token>> {
    let prefix = "auxdata:";
    if !instruction.starts_with(prefix) {
        return None;
    }

    let data = instruction.chars().skip(prefix.len()).collect::<String>();
    Some(vec![Token::constant(data[2..].to_owned())])
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
        let params = tokenize_params(&captures[2]);

        return Some(vec![params, f].concat());
    }

    None
}

fn tokenize_params(params: &str) -> Vec<Token> {
    static COMMA_SPACE: Lazy<Regex> = Lazy::new(|| Regex::new(r",\s*").unwrap());

    let params: Vec<&str> = COMMA_SPACE.split(params).collect();
    params
        .into_iter()
        .rev()
        .map(tokenize_part)
        .flatten()
        .collect()
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
        .split(" ")
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
    use super::{reduce_spaces, remove_comments};

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
}
