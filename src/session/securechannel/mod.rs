//! Implementation of the GlobalPlatform Secure Channel Protocol '03' (SCP03)
//!
//! See GPC_SPE_014: GlobalPlatform Card Technology Secure Channel Protocol '03' at:
//! <https://www.globalplatform.org/specificationscard.asp>
//!
//! SCP03 provides an encrypted channel using symmetric encryption alone.
//! AES-128-CBC is used for encryption, and AES-128-CMAC for authentication.
//!
//! While SCP03 is a multipurpose protocol, this implementation has been
//! written with the specific intention of communicating with Yubico's
//! YubiHSM2 devices and therefore omits certain features (e.g. additional
//! key sizes besides 128-bit) which are not relevant to the YubiHSM2 use case.
//!
//! It also follows the APDU format as described in Yubico's YubiHSM2
//! documentation as opposed to the one specified in GPC_SPE_014.
//!
//! For more information on the YubiHSM2 command format, see:
//!
//! <https://developers.yubico.com/YubiHSM2/Commands/>

mod challenge;
mod context;
mod cryptogram;
mod kdf;
mod mac;

/// AES key size in bytes. SCP03 theoretically supports other key sizes, but
/// the YubiHSM2 does not. Since this crate is somewhat specialized to the `YubiHSM2` (at least for now)
/// we hardcode to 128-bit for simplicity.
pub const KEY_SIZE: usize = 16;

pub(crate) use self::mac::{Mac, MAC_SIZE};
pub use self::{
    challenge::{Challenge, CHALLENGE_SIZE},
    context::{Context, CONTEXT_SIZE},
    cryptogram::{Cryptogram, CRYPTOGRAM_SIZE},
};

use super::{CommandMessage, ResponseMessage, SessionError, SessionErrorKind::*, SessionId};
use aes::{
    block_cipher_trait::{
        generic_array::{typenum::U16, GenericArray},
        BlockCipher,
    },
    Aes128,
};
use auth_key::AuthKey;
use block_modes::{block_padding::Iso7816, BlockMode, BlockModeIv, Cbc};
use byteorder::{BigEndian, ByteOrder};
use cmac::{crypto_mac::Mac as CryptoMac, Cmac};
use command::CommandCode;
#[cfg(feature = "mockhsm")]
use response::ResponseCode;
#[cfg(feature = "mockhsm")]
use subtle::ConstantTimeEq;
use zeroize::Zeroize;

/// Size of an AES block
const AES_BLOCK_SIZE: usize = 16;

/// SCP03 uses AES-128 encryption in CBC mode with ISO 7816 padding
type Aes128Cbc = Cbc<Aes128, Iso7816>;

/// SCP03 Secure Channel
pub(crate) struct SecureChannel {
    /// ID of this channel (a.k.a. session ID)
    id: SessionId,

    /// Number of messages sent over this channel
    counter: u32,

    /// External authentication state
    security_level: SecurityLevel,

    /// Context (card + host challenges)
    context: Context,

    /// Session encryption key (S-ENC)
    enc_key: [u8; KEY_SIZE],

    /// Session Command MAC key (S-MAC)
    mac_key: [u8; KEY_SIZE],

    /// Session Respose MAC key (S-RMAC)
    rmac_key: [u8; KEY_SIZE],

    /// Chaining value to be included when computing MACs
    mac_chaining_value: [u8; MAC_SIZE * 2],
}

impl SecureChannel {
    /// Create a new channel with the given ID, auth key, and host/card challenges
    pub fn new(
        id: SessionId,
        auth_key: &AuthKey,
        host_challenge: Challenge,
        card_challenge: Challenge,
    ) -> Self {
        let context = Context::from_challenges(host_challenge, card_challenge);
        let enc_key = derive_key(auth_key.enc_key(), 0b100, &context);
        let mac_key = derive_key(auth_key.mac_key(), 0b110, &context);
        let rmac_key = derive_key(auth_key.mac_key(), 0b111, &context);
        let mac_chaining_value = [0u8; MAC_SIZE * 2];

        Self {
            id,
            counter: 0,
            security_level: SecurityLevel::None,
            context,
            enc_key,
            mac_key,
            rmac_key,
            mac_chaining_value,
        }
    }

    /// Get the channel (i.e. session) ID
    pub fn id(&self) -> SessionId {
        self.id
    }

    /// Calculate the card's cryptogram for this session
    pub fn card_cryptogram(&self) -> Cryptogram {
        let mut result_bytes = [0u8; CRYPTOGRAM_SIZE];
        kdf::derive(&self.mac_key, 0, &self.context, &mut result_bytes);

        let result = Cryptogram::from_slice(&result_bytes);
        result_bytes.zeroize();

        result
    }

    /// Calculate the host's cryptogram for this session
    pub fn host_cryptogram(&self) -> Cryptogram {
        let mut result_bytes = [0u8; CRYPTOGRAM_SIZE];
        kdf::derive(&self.mac_key, 1, &self.context, &mut result_bytes);

        let result = Cryptogram::from_slice(&result_bytes);
        result_bytes.zeroize();

        result
    }

    /// Compute a command message with a MAC value for this session
    pub fn command_with_mac(
        &mut self,
        command_type: CommandCode,
        command_data: &[u8],
    ) -> Result<CommandMessage, SessionError> {
        if self.counter >= MAX_COMMANDS_PER_SESSION {
            self.terminate();
            fail!(
                CommandLimitExceeded,
                "max of {} command per session exceeded",
                MAX_COMMANDS_PER_SESSION
            );
        }

        let mut mac = Cmac::<Aes128>::new_varkey(self.mac_key.as_ref()).unwrap();
        mac.input(&self.mac_chaining_value);
        mac.input(&[command_type.to_u8()]);

        let mut length = [0u8; 2];
        BigEndian::write_u16(&mut length, (1 + command_data.len() + MAC_SIZE) as u16);
        mac.input(&length);
        mac.input(&[self.id.to_u8()]);
        mac.input(command_data);

        let tag = mac.result().code();
        self.mac_chaining_value.copy_from_slice(tag.as_slice());

        Ok(CommandMessage::new_with_mac(
            command_type,
            self.id,
            command_data,
            &tag,
        )?)
    }

    /// Compute a message for authenticating the host to the card
    pub fn authenticate_session(&mut self) -> Result<CommandMessage, SessionError> {
        assert_eq!(self.security_level, SecurityLevel::None);
        assert_eq!(self.mac_chaining_value, [0u8; MAC_SIZE * 2]);

        let host_cryptogram = self.host_cryptogram();
        self.command_with_mac(CommandCode::AuthSession, host_cryptogram.as_slice())
    }

    /// Handle the authenticate session response from the card
    pub fn finish_authenticate_session(
        &mut self,
        response: &ResponseMessage,
    ) -> Result<(), SessionError> {
        // The EXTERNAL_AUTHENTICATE command does not send an R-MAC value
        if !response.data.is_empty() {
            self.terminate();
            fail!(
                ProtocolError,
                "expected empty response data (got {}-bytes)",
                response.data.len(),
            );
        }

        self.security_level = SecurityLevel::Authenticated;

        // "The encryption counter’s start value shall be set to 1 for the
        // first command following a successful EXTERNAL AUTHENTICATE
        // command." -- GPC_SPE_014 section 6.2.6
        self.counter = 1;

        Ok(())
    }

    /// Encrypt a command to be sent to the card
    pub fn encrypt_command(
        &mut self,
        command: CommandMessage,
    ) -> Result<CommandMessage, SessionError> {
        assert_eq!(self.security_level, SecurityLevel::Authenticated);

        let mut message: Vec<u8> = command.into();
        let pos = message.len();

        // Provide space at the end of the vec for the padding
        message.extend_from_slice(&[0u8; AES_BLOCK_SIZE]);

        let cipher = Aes128::new_varkey(&self.enc_key).unwrap();
        let icv = compute_icv(&cipher, self.counter);
        let cbc_encryptor = Aes128Cbc::new(cipher, &icv);
        let ciphertext = cbc_encryptor.encrypt_pad(&mut message, pos).unwrap();

        self.command_with_mac(CommandCode::SessionMessage, ciphertext)
    }

    /// Verify and decrypt a response from the card
    pub fn decrypt_response(
        &mut self,
        encrypted_response: ResponseMessage,
    ) -> Result<ResponseMessage, SessionError> {
        assert_eq!(self.security_level, SecurityLevel::Authenticated);

        let cipher = Aes128::new_varkey(&self.enc_key).unwrap();
        let icv = compute_icv(&cipher, self.counter);

        self.verify_response_mac(&encrypted_response)?;

        let cipher = Aes128::new_varkey(&self.enc_key).unwrap();
        let cbc_decryptor = Aes128Cbc::new(cipher, &icv);

        let mut response_message = encrypted_response.data;
        let response_len = cbc_decryptor
            .decrypt_pad(&mut response_message)
            .map_err(|e| {
                self.terminate();
                err!(ProtocolError, "error decrypting response: {:?}", e)
            })?.len();

        response_message.truncate(response_len);
        let mut decrypted_response = ResponseMessage::parse(response_message)?;
        decrypted_response.session_id = encrypted_response.session_id;

        Ok(decrypted_response)
    }

    /// Ensure message authenticity by verifying the response MAC (R-MAC) sent from the card
    pub fn verify_response_mac(&mut self, response: &ResponseMessage) -> Result<(), SessionError> {
        assert_eq!(self.security_level, SecurityLevel::Authenticated);

        let session_id = response.session_id.ok_or_else(|| {
            self.terminate();
            err!(ProtocolError, "no session ID in response")
        })?;

        if self.id != session_id {
            self.terminate();
            fail!(
                MismatchError,
                "message has session ID {} (expected {})",
                session_id.to_u8(),
                self.id.to_u8(),
            );
        }

        let mut mac = Cmac::<Aes128>::new_varkey(self.rmac_key.as_ref()).unwrap();
        mac.input(&self.mac_chaining_value);
        mac.input(&[response.code.to_u8()]);

        let mut length = [0u8; 2];
        BigEndian::write_u16(&mut length, response.len() as u16);
        mac.input(&length);
        mac.input(&[session_id.to_u8()]);
        mac.input(&response.data);

        if response
            .mac
            .as_ref()
            .expect("missing R-MAC tag!")
            .verify(&mac.result().code())
            .is_err()
        {
            self.terminate();
            fail!(VerifyFailed, "R-MAC mismatch!");
        }

        self.increment_counter();
        Ok(())
    }

    /// Verify a host authentication message (for simulating a connector/card)
    #[cfg(feature = "mockhsm")]
    pub fn verify_authenticate_session(
        &mut self,
        command: &CommandMessage,
    ) -> Result<ResponseMessage, SessionError> {
        assert_eq!(self.security_level, SecurityLevel::None);
        assert_eq!(self.mac_chaining_value, [0u8; MAC_SIZE * 2]);

        if command.data.len() != CRYPTOGRAM_SIZE {
            self.terminate();
            fail!(
                ProtocolError,
                "expected {}-byte command data (got {})",
                CRYPTOGRAM_SIZE,
                command.data.len()
            );
        }

        let expected_host_cryptogram = self.host_cryptogram();
        let actual_host_cryptogram = Cryptogram::from_slice(&command.data);

        if expected_host_cryptogram
            .ct_eq(&actual_host_cryptogram)
            .unwrap_u8()
            != 1
        {
            self.terminate();
            fail!(VerifyFailed, "host cryptogram mismatch!");
        }

        self.verify_command_mac(command)?;
        self.security_level = SecurityLevel::Authenticated;

        // "The encryption counter’s start value shall be set to 1 for the
        // first command following a successful EXTERNAL AUTHENTICATE
        // command." -- GPC_SPE_014 section 6.2.6
        self.counter = 1;

        Ok(ResponseMessage::success(CommandCode::AuthSession, vec![]))
    }

    /// Verify and decrypt a command from the host
    #[cfg(feature = "mockhsm")]
    pub fn decrypt_command(
        &mut self,
        encrypted_command: CommandMessage,
    ) -> Result<CommandMessage, SessionError> {
        assert_eq!(self.security_level, SecurityLevel::Authenticated);

        let cipher = Aes128::new_varkey(&self.enc_key).unwrap();
        let icv = compute_icv(&cipher, self.counter);

        self.verify_command_mac(&encrypted_command)?;

        let cipher = Aes128::new_varkey(&self.enc_key).unwrap();
        let cbc_decryptor = Aes128Cbc::new(cipher, &icv);

        let mut command_data = encrypted_command.data;
        let command_len = cbc_decryptor
            .decrypt_pad(&mut command_data)
            .map_err(|e| {
                self.terminate();
                err!(ProtocolError, "error decrypting command: {:?}", e)
            })?.len();

        command_data.truncate(command_len);
        let mut decrypted_command = CommandMessage::parse(command_data)?;
        decrypted_command.session_id = encrypted_command.session_id;

        Ok(decrypted_command)
    }

    /// Verify a Command MAC (C-MAC) value, updating the internal session state
    #[cfg(feature = "mockhsm")]
    pub fn verify_command_mac(&mut self, command: &CommandMessage) -> Result<(), SessionError> {
        assert_eq!(
            command.session_id.unwrap(),
            self.id,
            "session ID mismatch: {:?}",
            command.session_id
        );

        let mut mac = Cmac::<Aes128>::new_varkey(self.mac_key.as_ref()).unwrap();
        mac.input(&self.mac_chaining_value);
        mac.input(&[command.command_type.to_u8()]);

        let mut length = [0u8; 2];
        BigEndian::write_u16(&mut length, command.len() as u16);
        mac.input(&length);
        mac.input(&[command.session_id.unwrap().to_u8()]);
        mac.input(&command.data);

        let tag = mac.result().code();

        if command
            .mac
            .as_ref()
            .expect("missing C-MAC tag!")
            .verify(&tag)
            .is_err()
        {
            self.terminate();
            fail!(VerifyFailed, "C-MAC mismatch!");
        }

        self.mac_chaining_value.copy_from_slice(tag.as_slice());
        Ok(())
    }

    /// Encrypt a response to be sent back to the host
    #[cfg(feature = "mockhsm")]
    pub fn encrypt_response(
        &mut self,
        response: ResponseMessage,
    ) -> Result<ResponseMessage, SessionError> {
        assert_eq!(self.security_level, SecurityLevel::Authenticated);

        let mut message: Vec<u8> = response.into();
        let pos = message.len();

        // Provide space at the end of the vec for the padding
        message.extend_from_slice(&[0u8; AES_BLOCK_SIZE]);

        let cipher = Aes128::new_varkey(&self.enc_key).unwrap();
        let icv = compute_icv(&cipher, self.counter);
        let cbc_encryptor = Aes128Cbc::new(cipher, &icv);

        let ct_len = cbc_encryptor.encrypt_pad(&mut message, pos).unwrap().len();
        message.truncate(ct_len);

        self.response_with_mac(ResponseCode::Success(CommandCode::SessionMessage), message)
    }

    /// Compute the MAC for a response message
    #[cfg(feature = "mockhsm")]
    pub fn response_with_mac<T>(
        &mut self,
        code: ResponseCode,
        response_data: T,
    ) -> Result<ResponseMessage, SessionError>
    where
        T: Into<Vec<u8>>,
    {
        assert_eq!(self.security_level, SecurityLevel::Authenticated);
        let body = response_data.into();

        let mut mac = Cmac::<Aes128>::new_varkey(self.rmac_key.as_ref()).unwrap();
        mac.input(&self.mac_chaining_value);
        mac.input(&[code.to_u8()]);

        let mut length = [0u8; 2];
        BigEndian::write_u16(&mut length, (1 + body.len() + MAC_SIZE) as u16);
        mac.input(&length);
        mac.input(&[self.id.to_u8()]);
        mac.input(&body);

        self.increment_counter();

        Ok(ResponseMessage::new_with_mac(
            code,
            self.id,
            body,
            &mac.result().code(),
        ))
    }

    /// Get the current value of the internal message counter
    pub(super) fn counter(&self) -> usize {
        self.counter as usize
    }

    /// Increment the internal message counter
    fn increment_counter(&mut self) {
        self.counter = self.counter.checked_add(1).unwrap_or_else(|| {
            // We should always hit MAX_COMMANDS_PER_SESSION before this
            // happens unless there is a bug.
            panic!("session counter overflowed!");
        });
    }

    /// Terminate the session
    fn terminate(&mut self) {
        self.security_level = SecurityLevel::Terminated;
        self.enc_key.zeroize();
        self.mac_key.zeroize();
        self.rmac_key.zeroize();
    }
}

impl Drop for SecureChannel {
    fn drop(&mut self) {
        self.terminate();
    }
}

/// Maximum number of messages allowed in a single session: 2^20.
///
/// This is a conservative number chosen due to the small MAC size used by
/// the SCP03 protocol: 8-bytes. This small tag has an even smaller birthday
/// bound on collisions, so to avoid these we force generation of fresh
/// session keys after the following number of messages have been sent.
pub const MAX_COMMANDS_PER_SESSION: u32 = 0x10_0000;

/// Current Security Level: protocol state
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SecurityLevel {
    /// 'NO_SECURITY_LEVEL' i.e. session is terminated or not fully initialized
    None,

    /// 'AUTHENTICATED' i.e. the EXTERNAL_AUTHENTICATE command has completed
    Authenticated,

    /// Terminated: either explicitly closed or due to protocol error
    Terminated,
}

/// Derive a key using the SCP03 KDF
fn derive_key(parent_key: &[u8], derivation_constant: u8, context: &Context) -> [u8; KEY_SIZE] {
    let mut key = [0u8; KEY_SIZE];
    kdf::derive(parent_key, derivation_constant, context, &mut key);
    key
}

/// Compute an "Initial Chaining Vector" (ICV) from a counter
fn compute_icv(cipher: &Aes128, counter: u32) -> GenericArray<u8, U16> {
    // "Initial Chaining Vector" - CBC IVs generated from encrypting a counter
    let mut icv = GenericArray::clone_from_slice(&[0u8; AES_BLOCK_SIZE]);
    BigEndian::write_u32(&mut icv.as_mut_slice()[12..], counter);
    cipher.encrypt_block(&mut icv);
    icv
}

#[cfg(all(test, feature = "mockhsm"))]
mod tests {
    use super::*;
    use auth_key::AuthKey;
    use command::CommandCode;

    const PASSWORD: &[u8] = b"password";
    const HOST_CHALLENGE: &[u8] = &[0u8; 8];
    const CARD_CHALLENGE: &[u8] = &[0u8; 8];
    const COMMAND_CODE: CommandCode = CommandCode::Echo;
    const COMMAND_DATA: &[u8] = b"Hello, world!";

    fn create_channel_pair() -> (SecureChannel, SecureChannel) {
        let auth_key = AuthKey::derive_from_password(PASSWORD);

        let host_challenge = Challenge::from_slice(HOST_CHALLENGE);
        let card_challenge = Challenge::from_slice(CARD_CHALLENGE);

        let session_id = SessionId::new(0).unwrap();

        // Create channels
        let mut host_channel =
            SecureChannel::new(session_id, &auth_key, host_challenge, card_challenge);
        let mut card_channel =
            SecureChannel::new(session_id, &auth_key, host_challenge, card_challenge);

        // Auth host to card
        let auth_command = host_channel.authenticate_session().unwrap();
        let auth_response = card_channel
            .verify_authenticate_session(&auth_command)
            .unwrap();

        host_channel
            .finish_authenticate_session(&auth_response)
            .unwrap();

        (host_channel, card_channel)
    }

    #[test]
    fn happy_path_test() {
        let (mut host_channel, mut card_channel) = create_channel_pair();

        // Host sends encrypted command
        let command_ciphertext = host_channel
            .encrypt_command(CommandMessage::new(COMMAND_CODE, Vec::from(COMMAND_DATA)).unwrap())
            .unwrap();

        // Card decrypts command
        let decrypted_command = card_channel.decrypt_command(command_ciphertext).unwrap();

        // Card sends decrypted response
        let response_ciphertext = card_channel
            .encrypt_response(ResponseMessage::success(
                decrypted_command.command_type,
                decrypted_command.data,
            )).unwrap();

        let decrypted_response = host_channel.decrypt_response(response_ciphertext).unwrap();

        assert_eq!(host_channel.security_level, SecurityLevel::Authenticated);
        assert_eq!(decrypted_response.command().unwrap(), COMMAND_CODE);
        assert_eq!(&decrypted_response.data[..], COMMAND_DATA);
    }

    #[test]
    fn mac_verify_failure_test() {
        let (mut host_channel, mut card_channel) = create_channel_pair();

        // Host sends encrypted command
        let command_ciphertext = host_channel
            .encrypt_command(CommandMessage::new(COMMAND_CODE, Vec::from(COMMAND_DATA)).unwrap())
            .unwrap();

        // Card decrypts command
        let decrypted_command = card_channel.decrypt_command(command_ciphertext).unwrap();

        // Card sends decrypted response
        let mut response_ciphertext = card_channel
            .encrypt_response(ResponseMessage::success(
                decrypted_command.command_type,
                decrypted_command.data,
            )).unwrap();

        // Tweak MAC in response
        let mut bad_mac = Vec::from(response_ciphertext.mac.as_ref().unwrap().as_slice());
        bad_mac[0] ^= 0xAA;
        response_ciphertext.mac = Some(Mac::from_slice(&bad_mac));

        let response = host_channel.decrypt_response(response_ciphertext);
        assert!(response.is_err());
        assert_eq!(host_channel.security_level, SecurityLevel::Terminated);
        assert_eq!(
            response.err().unwrap().to_string(),
            "verification failed: R-MAC mismatch!"
        );
    }
}
