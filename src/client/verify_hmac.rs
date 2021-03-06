//! Verify HMAC tag for the given input data
//!
//! <https://developers.yubico.com/YubiHSM2/Commands/Verify_Hmac.html>

use super::hmac::HMACTag;
use command::{Command, CommandCode};
use object::ObjectId;
use response::Response;

/// Request parameters for `command::hmac`
#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct VerifyHMACCommand {
    /// ID of the key to verify the HMAC tag with
    pub key_id: ObjectId,

    /// HMAC tag to be verified
    pub tag: HMACTag,

    /// Data to be authenticated
    pub data: Vec<u8>,
}

impl Command for VerifyHMACCommand {
    type ResponseType = VerifyHMACResponse;
}

/// HMAC tags
#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct VerifyHMACResponse(pub(crate) u8);

impl Response for VerifyHMACResponse {
    const COMMAND_CODE: CommandCode = CommandCode::VerifyHMAC;
}
