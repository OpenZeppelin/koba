mod instruction;
mod labeler;
mod tokenizer;

pub use instruction::{instruction, opcode};

pub fn compile(evmasm: &str, wasm: &[u8]) -> eyre::Result<String> {
    let evmasm = tokenizer::amend(evmasm, wasm);
    let bytecode = codegen(&evmasm)?;
    Ok(hex::encode(bytecode))
}

fn codegen(asm: &str) -> eyre::Result<Vec<u8>> {
    let instructions = tokenizer::clean_asm(asm);
    let stream = tokenizer::tokenize(instructions);
    let stream = labeler::labelize(&stream);

    let mut bytecode = Vec::with_capacity(stream.len() * 2);
    for token in stream {
        let bytes = token.bytecode()?;
        bytecode.extend(bytes);
    }

    Ok(bytecode)
}

#[cfg(test)]
mod tests {
    use crate::assembler::codegen;

    use super::{
        labeler, opcode,
        tokenizer::{self, push_constant, Token},
    };

    fn to_hex_code(evmasm: &str) -> Vec<Token> {
        let instructions = tokenizer::clean_asm(&evmasm);
        let stream = tokenizer::tokenize(instructions);
        let stream = labeler::labelize(&stream);
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
        expected.extend(push_constant(
            "0000000000000000000000000000000000000000000000000000000000000028",
        ));
        expected.push(op!("DUP2"));
        expected.push(op!("DUP3"));
        expected.push(op!("JUMPDEST"));
        expected.push(op!("DUP4"));
        expected.extend(push_constant("01"));
        expected.push(op!("DUP5"));

        assert_eq!(expected, actual);
    }

    #[test]
    fn compiles() {
        let asm = r##"
    /* "src/proxy.sol":123:1084  contract Proxy {... */
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
        dataSize(sub_0)
        dup1
        dataOffset(sub_0)
        0x00
        codecopy
        0x00
        return
      invalid
      
      sub_0: assembly {
              /* "src/proxy.sol":123:1084  contract Proxy {... */
            mstore(0x40, 0x80)
            jumpi(tag_2, calldatasize)
              /* "src/proxy.sol":950:951  0 */
            0x00
            dup1
            dup2
            dup3
            dup4
              /* "src/proxy.sol":864:875  callvalue() */
            callvalue
              /* "src/proxy.sol":804:846  0xA6014eee4c8316f19E89E721a0e46Dd0704201FA */
            0xa6014eee4c8316f19e89e721a0e46dd0704201fa
              /* "src/proxy.sol":744:749  gas() */
            gas
              /* "src/proxy.sol":722:965  call(... */
            call
              /* "src/proxy.sol":979:1040  if eq(result, 0) {... */
            tag_5
            jumpi
              /* "src/proxy.sol":950:951  0 */
            dup1
            dup2
              /* "src/proxy.sol":1014:1026  revert(0, 0) */
            revert
              /* "src/proxy.sol":979:1040  if eq(result, 0) {... */
          tag_5:
              /* "src/proxy.sol":950:951  0 */
            dup1
            dup2
              /* "src/proxy.sol":1054:1066  return(0, 0) */
            return
              /* "src/proxy.sol":123:1084  contract Proxy {... */
          tag_2:
              /* "src/proxy.sol":223:226  0x0 */
            0x00
              /* "src/proxy.sol":228:242  calldatasize() */
            calldatasize
              /* "src/proxy.sol":223:226  0x0 */
            dup2
            dup3
              /* "src/proxy.sol":205:243  calldatacopy(0x0, 0x0, calldatasize()) */
            calldatacopy
              /* "src/proxy.sol":223:226  0x0 */
            dup1
            dup2
              /* "src/proxy.sol":228:242  calldatasize() */
            calldatasize
              /* "src/proxy.sol":223:226  0x0 */
            dup4
            dup5
              /* "src/proxy.sol":357:399  0x3323B6c94847d1Cf98AaE1ac0A1d745d3AF91e5E */
            0x3323b6c94847d1cf98aae1ac0a1d745d3af91e5e
              /* "src/proxy.sol":297:302  gas() */
            gas
              /* "src/proxy.sol":271:525  callcode(... */
            callcode
              /* "src/proxy.sol":539:600  if eq(result, 0) {... */
            tag_5
            jumpi
              /* "src/proxy.sol":223:226  0x0 */
            dup1
            dup2
              /* "src/proxy.sol":574:586  revert(0, 0) */
            revert
      
          auxdata: 0xa164736f6c6343000807000a
      }      
    "##;

        let expected = "6080604052348015600e575f80fd5b507f000000000000000000000000000000000000000000000000000000000000009d8060395f395ff3fe6080604052366030575f808182833473a6014eee4c8316f19e89e721a0e46dd0704201fa5af1602c578081fd5b8081f35b5f368182378081368384733323b6c94847d1cf98aae1ac0a1d745d3af91e5e5af2602c578081fda164736f6c6343000807000a";
        let actual = hex::encode(codegen(asm).unwrap());
        assert_eq!(expected, actual);
    }
}
