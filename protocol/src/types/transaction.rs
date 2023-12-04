pub use ethereum::{
    AccessList, AccessListItem, EIP1559TransactionMessage as TransactionMessage, TransactionAction,
    TransactionRecoveryId, TransactionSignature,
};
use rlp::{Encodable, RlpStream};
use serde::{Deserialize, Serialize};

use common_crypto::secp256k1_recover;

use crate::types::{
    Bloom, Bytes, BytesMut, CellDepWithPubKey, ExitReason, Hash, Hasher, Public, TxResp,
    TypesError, H160, H256, H520, U256, U64,
};
use crate::ProtocolResult;

pub const MAX_PRIORITY_FEE_PER_GAS: u64 = 1_337;

#[derive(Serialize, Deserialize, Clone, Debug, Hash, PartialEq, Eq)]
pub enum UnsignedTransaction {
    Legacy(LegacyTransaction),
    Eip2930(Eip2930Transaction),
    Eip1559(Eip1559Transaction),
}

impl UnsignedTransaction {
    pub fn type_(&self) -> u64 {
        match self {
            UnsignedTransaction::Legacy(_) => 0x00,
            UnsignedTransaction::Eip2930(_) => 0x01,
            UnsignedTransaction::Eip1559(_) => 0x02,
        }
    }

    pub fn may_cost(&self) -> ProtocolResult<U256> {
        if let Some(res) = U256::from(self.gas_price().low_u64())
            .checked_mul(U256::from(self.gas_limit().low_u64()))
        {
            return Ok(res
                .checked_add(*self.value())
                .unwrap_or_else(U256::max_value));
        }

        Err(TypesError::PrepayGasIsTooLarge.into())
    }

    pub fn is_legacy(&self) -> bool {
        matches!(self, UnsignedTransaction::Legacy(_))
    }

    pub fn is_eip1559(&self) -> bool {
        matches!(self, UnsignedTransaction::Eip1559(_))
    }

    pub fn data(&self) -> &[u8] {
        match self {
            UnsignedTransaction::Legacy(tx) => tx.data.as_ref(),
            UnsignedTransaction::Eip2930(tx) => tx.data.as_ref(),
            UnsignedTransaction::Eip1559(tx) => tx.data.as_ref(),
        }
    }

    pub fn set_action(&mut self, action: TransactionAction) {
        match self {
            UnsignedTransaction::Legacy(tx) => tx.action = action,
            UnsignedTransaction::Eip2930(tx) => tx.action = action,
            UnsignedTransaction::Eip1559(tx) => tx.action = action,
        }
    }

    pub fn set_data(&mut self, data: Bytes) {
        match self {
            UnsignedTransaction::Legacy(tx) => tx.data = data,
            UnsignedTransaction::Eip2930(tx) => tx.data = data,
            UnsignedTransaction::Eip1559(tx) => tx.data = data,
        }
    }

    pub fn gas_price(&self) -> U64 {
        match self {
            UnsignedTransaction::Legacy(tx) => tx.gas_price,
            UnsignedTransaction::Eip2930(tx) => tx.gas_price,
            UnsignedTransaction::Eip1559(tx) => tx.gas_price.max(tx.max_priority_fee_per_gas),
        }
    }

    pub fn max_priority_fee_per_gas(&self) -> &U64 {
        match self {
            UnsignedTransaction::Legacy(tx) => &tx.gas_price,
            UnsignedTransaction::Eip2930(tx) => &tx.gas_price,
            UnsignedTransaction::Eip1559(tx) => &tx.max_priority_fee_per_gas,
        }
    }

    pub fn get_legacy(&self) -> Option<LegacyTransaction> {
        match self {
            UnsignedTransaction::Legacy(tx) => Some(tx.clone()),
            _ => None,
        }
    }

    pub fn as_u8(&self) -> u8 {
        match self {
            UnsignedTransaction::Legacy(_) => unreachable!(),
            UnsignedTransaction::Eip2930(_) => 1u8,
            UnsignedTransaction::Eip1559(_) => 2u8,
        }
    }

    pub fn encode(
        &self,
        chain_id: Option<u64>,
        signature: Option<SignatureComponents>,
    ) -> BytesMut {
        UnverifiedTransaction {
            unsigned: self.clone(),
            chain_id,
            signature,
            hash: Default::default(),
        }
        .rlp_bytes()
    }

    pub fn to(&self) -> Option<H160> {
        match self {
            UnsignedTransaction::Legacy(tx) => tx.get_to(),
            UnsignedTransaction::Eip2930(tx) => tx.get_to(),
            UnsignedTransaction::Eip1559(tx) => tx.get_to(),
        }
    }

    pub fn value(&self) -> &U256 {
        match self {
            UnsignedTransaction::Legacy(tx) => &tx.value,
            UnsignedTransaction::Eip2930(tx) => &tx.value,
            UnsignedTransaction::Eip1559(tx) => &tx.value,
        }
    }

    pub fn gas_limit(&self) -> &U64 {
        match self {
            UnsignedTransaction::Legacy(tx) => &tx.gas_limit,
            UnsignedTransaction::Eip2930(tx) => &tx.gas_limit,
            UnsignedTransaction::Eip1559(tx) => &tx.gas_limit,
        }
    }

    pub fn nonce(&self) -> &U64 {
        match self {
            UnsignedTransaction::Legacy(tx) => &tx.nonce,
            UnsignedTransaction::Eip2930(tx) => &tx.nonce,
            UnsignedTransaction::Eip1559(tx) => &tx.nonce,
        }
    }

    pub fn action(&self) -> &TransactionAction {
        match self {
            UnsignedTransaction::Legacy(tx) => &tx.action,
            UnsignedTransaction::Eip2930(tx) => &tx.action,
            UnsignedTransaction::Eip1559(tx) => &tx.action,
        }
    }

    pub fn access_list(&self) -> AccessList {
        match self {
            UnsignedTransaction::Legacy(_) => Vec::new(),
            UnsignedTransaction::Eip2930(tx) => tx.access_list.clone(),
            UnsignedTransaction::Eip1559(tx) => tx.access_list.clone(),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct LegacyTransaction {
    /// According to [EIP-2681](https://eips.ethereum.org/EIPS/eip-2681),
    /// limit account nonce to 2^64-1.
    pub nonce:     U64,
    pub gas_price: U64,
    pub gas_limit: U64,
    pub action:    TransactionAction,
    pub value:     U256,
    pub data:      Bytes,
}

impl std::hash::Hash for LegacyTransaction {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.nonce.hash(state);
        self.gas_price.hash(state);
        self.gas_limit.hash(state);
        self.value.hash(state);
        self.data.hash(state);
        if let TransactionAction::Call(addr) = self.action {
            addr.hash(state);
        }
    }
}

impl LegacyTransaction {
    pub fn get_to(&self) -> Option<H160> {
        match self.action {
            TransactionAction::Call(to) => Some(to),
            TransactionAction::Create => None,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Eip2930Transaction {
    /// According to [EIP-2681](https://eips.ethereum.org/EIPS/eip-2681),
    /// limit account nonce to 2^64-1.
    pub nonce:       U64,
    pub gas_price:   U64,
    pub gas_limit:   U64,
    pub action:      TransactionAction,
    pub value:       U256,
    pub data:        Bytes,
    pub access_list: AccessList,
}

impl std::hash::Hash for Eip2930Transaction {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.nonce.hash(state);
        self.gas_price.hash(state);
        self.gas_limit.hash(state);
        self.value.hash(state);
        self.data.hash(state);
        if let TransactionAction::Call(addr) = self.action {
            addr.hash(state);
        }

        for access in self.access_list.iter() {
            access.address.hash(state);
        }
    }
}

impl Eip2930Transaction {
    pub fn get_to(&self) -> Option<H160> {
        match self.action {
            TransactionAction::Call(to) => Some(to),
            TransactionAction::Create => None,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Eip1559Transaction {
    pub nonce:                    U64,
    pub max_priority_fee_per_gas: U64,
    pub gas_price:                U64,
    pub gas_limit:                U64,
    pub action:                   TransactionAction,
    pub value:                    U256,
    pub data:                     Bytes,
    pub access_list:              AccessList,
}

impl std::hash::Hash for Eip1559Transaction {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.nonce.hash(state);
        self.max_priority_fee_per_gas.hash(state);
        self.gas_price.hash(state);
        self.gas_limit.hash(state);
        self.value.hash(state);
        self.data.hash(state);
        if let TransactionAction::Call(addr) = self.action {
            addr.hash(state);
        }

        for access in self.access_list.iter() {
            access.address.hash(state);
        }
    }
}

impl Eip1559Transaction {
    pub fn get_to(&self) -> Option<H160> {
        match self.action {
            TransactionAction::Call(to) => Some(to),
            TransactionAction::Create => None,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, Hash, PartialEq, Eq)]
pub struct UnverifiedTransaction {
    pub unsigned:  UnsignedTransaction,
    pub signature: Option<SignatureComponents>,
    pub chain_id:  Option<u64>,
    pub hash:      H256,
}

impl UnverifiedTransaction {
    pub fn calc_hash(mut self) -> Self {
        debug_assert!(self.signature.is_some());
        let hash = self.get_hash();
        self.hash = hash;
        self
    }

    pub fn get_hash(&self) -> H256 {
        Hasher::digest(&self.unsigned.encode(self.chain_id, self.signature.clone()))
    }

    pub fn check_hash(&self) -> ProtocolResult<()> {
        let calc_hash = self.get_hash();
        if self.hash != calc_hash {
            return Err(TypesError::TxHashMismatch {
                origin: self.hash,
                calc:   calc_hash,
            }
            .into());
        }

        Ok(())
    }

    /// The `with_chain_id` argument is only used for tests
    pub fn signature_hash(&self, with_chain_id: bool) -> Hash {
        if !with_chain_id {
            if let Some(legacy_tx) = self.unsigned.get_legacy() {
                let mut s = RlpStream::new();
                legacy_tx.rlp_encode(&mut s, None, None);
                return Hasher::digest(s.out());
            }
        }

        Hasher::digest(self.unsigned.encode(self.chain_id, None))
    }

    pub fn recover_public(&self, with_chain_id: bool) -> ProtocolResult<Public> {
        Ok(Public::from_slice(
            &secp256k1_recover(
                self.signature_hash(with_chain_id).as_bytes(),
                self.signature
                    .as_ref()
                    .ok_or(TypesError::MissingSignature)?
                    .as_bytes()
                    .as_ref(),
            )
            .map_err(TypesError::Crypto)?
            .serialize_uncompressed()[1..65],
        ))
    }
}

#[derive(Serialize, Deserialize, Default, Clone, Debug, Hash, PartialEq, Eq)]
pub struct SignatureComponents {
    pub r:          Bytes,
    pub s:          Bytes,
    pub standard_v: u8,
}

/// This is only use for test.
impl From<Bytes> for SignatureComponents {
    // assume that all the bytes data are in Ethereum-like format
    fn from(bytes: Bytes) -> Self {
        debug_assert!(bytes.len() == 65);
        SignatureComponents {
            r:          Bytes::from(bytes[0..32].to_vec()),
            s:          Bytes::from(bytes[32..64].to_vec()),
            standard_v: bytes[64],
        }
    }
}

impl From<SignatureComponents> for Bytes {
    fn from(sc: SignatureComponents) -> Self {
        let mut bytes = BytesMut::from(sc.r.as_ref());
        bytes.extend_from_slice(sc.s.as_ref());
        bytes.extend_from_slice(&[sc.standard_v]);
        bytes.freeze()
    }
}

impl SignatureComponents {
    pub const SECP256K1_SIGNATURE_LEN: usize = 65;

    pub fn as_bytes(&self) -> Bytes {
        self.clone().into()
    }

    pub fn is_eth_sig(&self) -> bool {
        self.len() == Self::SECP256K1_SIGNATURE_LEN
    }

    pub fn add_chain_replay_protection(&self, chain_id: Option<u64>) -> u64 {
        (self.standard_v as u64) + chain_id.map(|i| i * 2 + 35).unwrap_or(27)
    }

    pub fn extract_standard_v(v: u64) -> Option<u8> {
        match v {
            v if v == 27 => Some(0),
            v if v == 28 => Some(1),
            v if v >= 35 => Some(((v - 1) % 2) as u8),
            _ => None,
        }
    }

    pub fn extract_chain_id(v: u64) -> Option<u64> {
        if v >= 35 {
            Some((v - 35) / 2u64)
        } else {
            None
        }
    }

    pub(crate) fn extract_interoperation_tx_sender(&self) -> ProtocolResult<H160> {
        // Only call CKB-VM mode is supported now
        if self.r[0] == 0 {
            let r = rlp::decode::<CellDepWithPubKey>(&self.r[1..])
                .map_err(TypesError::DecodeInteroperationSigR)?;

            return Ok(Hasher::digest(&r.pub_key).into());
        }

        Err(TypesError::InvalidSignatureRType.into())
    }

    #[allow(clippy::len_without_is_empty)]
    pub fn len(&self) -> usize {
        self.r.len() + self.s.len() + 1
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, Hash, PartialEq, Eq)]
pub struct SignedTransaction {
    pub transaction: UnverifiedTransaction,
    pub sender:      H160,
    pub public:      Option<Public>,
}

impl SignedTransaction {
    pub fn from_unverified(utx: UnverifiedTransaction) -> ProtocolResult<Self> {
        if utx.signature.is_none() {
            return Err(TypesError::Unsigned.into());
        }

        let hash = utx.signature_hash(true);
        let sig = utx.signature.as_ref().unwrap();

        if sig.is_eth_sig() {
            let public = Public::from_slice(
                &secp256k1_recover(hash.as_bytes(), sig.as_bytes().as_ref())
                    .map_err(TypesError::Crypto)?
                    .serialize_uncompressed()[1..65],
            );

            return Ok(SignedTransaction {
                transaction: utx.calc_hash(),
                sender:      public_to_address(&public),
                public:      Some(public),
            });
        }

        // Otherwise it is an interoperation transaction
        Ok(SignedTransaction {
            sender:      sig.extract_interoperation_tx_sender()?,
            public:      Some(Public::zero()),
            transaction: utx.calc_hash(),
        })
    }

    pub fn type_(&self) -> u64 {
        self.transaction.unsigned.type_()
    }

    pub fn get_to(&self) -> Option<H160> {
        self.transaction.unsigned.to()
    }

    pub fn is_eip155(&self) -> bool {
        self.transaction.chain_id.is_some()
    }

    /// Encode a transaction receipt into bytes.
    ///
    /// According to [`EIP-2718`]:
    /// - `Receipt` is either `TransactionType || ReceiptPayload` or
    ///   `LegacyReceipt`.
    /// - `LegacyReceipt` is kept to be RLP encoded bytes; it is `rlp([status,
    ///   cumulativeGasUsed, logsBloom, logs])`.
    /// - `ReceiptPayload` is an opaque byte array whose interpretation is
    ///   dependent on the `TransactionType` and defined in future EIPs.
    ///   - As [`EIP-2930`] defined: if `TransactionType` is `1`,
    ///     `ReceiptPayload` is `rlp([status, cumulativeGasUsed, logsBloom,
    ///     logs])`.
    ///   - As [`EIP-1559`] defined: if `TransactionType` is `2`,
    ///     `ReceiptPayload` is `rlp([status, cumulative_transaction_gas_used,
    ///     logs_bloom, logs])`.
    ///
    /// [`EIP-2718`]: https://eips.ethereum.org/EIPS/eip-2718#receipts
    /// [`EIP-2930`]: https://eips.ethereum.org/EIPS/eip-2930#parameters
    /// [`EIP-1559`]: https://eips.ethereum.org/EIPS/eip-1559#specification
    pub fn encode_receipt(&self, r: &TxResp, logs_bloom: Bloom) -> Bytes {
        // Status: either 1 (success) or 0 (failure).
        // Only present after activation of [EIP-658](https://eips.ethereum.org/EIPS/eip-658)
        let status: u64 = if matches!(r.exit_reason, ExitReason::Succeed(_)) {
            1
        } else {
            0
        };
        let used_gas = U256::from(r.gas_used);
        let legacy_receipt = {
            let mut rlp = RlpStream::new();
            rlp.begin_list(4);
            rlp.append(&status);
            rlp.append(&used_gas);
            rlp.append(&logs_bloom);
            rlp.append_list(&r.logs);
            rlp.out().freeze()
        };
        match self.type_() {
            x if x == 0x01 || x == 0x02 => [&x.to_be_bytes()[7..], &legacy_receipt].concat().into(),
            _ => legacy_receipt, // legacy (0x00) or undefined type
        }
    }
}

pub fn public_to_address(public: &Public) -> H160 {
    let hash = Hasher::digest(public);
    let mut ret = H160::zero();
    ret.as_bytes_mut().copy_from_slice(&hash[12..]);
    ret
}

pub fn recover_intact_pub_key(public: &Public) -> H520 {
    let mut inner = vec![4u8];
    inner.extend_from_slice(public.as_bytes());
    H520::from_slice(&inner[0..65])
}
