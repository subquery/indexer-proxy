// This file is part of SubQuery.

// Copyright (C) 2020-2022 SubQuery Pte Ltd authors & contributors
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

use crate::types::Result;
use web3::signing::{keccak256, recover};

pub fn eth_message(message: String) -> [u8; 32] {
    keccak256(format!("{}{}{}", "\x19Ethereum Signed Message:\n", message.len(), message).as_bytes())
}

pub fn recover_signer(message: String, signature: &str) -> Result<String> {
    let msg = eth_message(message);
    let sig = hex::decode(signature).unwrap();
    let recover_id = sig[64] as i32 - 27;
    let pubkey = recover(&msg, &sig[..64], recover_id).unwrap();
    let address = format!("{:02X?}", pubkey);

    Ok(address)
}
