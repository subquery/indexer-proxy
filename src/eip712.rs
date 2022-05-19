use crate::types::Result;
use web3::signing::{keccak256, recover};

pub fn eth_message(message: String) -> [u8; 32] {
    keccak256(
        format!(
            "{}{}{}",
            "\x19Ethereum Signed Message:\n",
            message.len(),
            message
        )
        .as_bytes(),
    )
}

pub fn recover_signer(message: String, signature: &str) -> Result<String> {
    let msg = eth_message(message);
    let sig = hex::decode(signature).unwrap();
    let recover_id = sig[64] as i32 - 27;
    let pubkey = recover(&msg, &sig[..64], recover_id).unwrap();
    let address = format!("{:02X?}", pubkey);

    Ok(address)
}
