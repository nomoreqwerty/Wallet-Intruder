use bip0039::{Count, English, Mnemonic};
use bitcoin::{
    secp256k1::{All, Secp256k1},
    Network,
};
use std::{
    fmt::Display,
    sync::Arc,
};

use crate::common::reusable::CommonDerivationPaths;

pub struct Wallet {
    pub mnemonic: Mnemonic,
    pub bip44_addr: String,
    pub bip49_addr: String,
    pub bip84_addr: String,
}

impl Wallet {
    pub fn generate(paths: &Arc<CommonDerivationPaths>, secp: &Arc<Secp256k1<All>>) -> Self {
        unsafe {
            let mnemonic: Mnemonic<English> = Mnemonic::generate(Count::Words12);

            let xprv = bitcoin::bip32::Xpriv::new_master(Network::Bitcoin, &mnemonic.to_seed(""))
                .unwrap_unchecked();

            let bip44_xprv = xprv.derive_priv(secp, &paths.bip44).unwrap_unchecked();
            let bip49_xprv = xprv.derive_priv(secp, &paths.bip49).unwrap_unchecked();
            let bip84_xprv = xprv.derive_priv(secp, &paths.bip84).unwrap_unchecked();

            let arc_secp = secp.clone();
            let (bip44_addr, bip49_addr, bip84_addr) = (
                bitcoin::Address::p2pkh(
                    &bip44_xprv.to_priv().public_key(&arc_secp),
                    Network::Bitcoin,
                ),
                bitcoin::Address::p2shwpkh(
                    &bip49_xprv.to_priv().public_key(&arc_secp),
                    Network::Bitcoin,
                )
                .unwrap(),
                bitcoin::Address::p2wpkh(
                    &bip84_xprv.to_priv().public_key(&arc_secp),
                    Network::Bitcoin,
                )
                .unwrap(),
            );

            Self {
                mnemonic,
                bip44_addr: bip44_addr.to_string(),
                bip49_addr: bip49_addr.to_string(),
                bip84_addr: bip84_addr.to_string(),
            }
        }
    }
}

pub enum AddressType {
    BIP44,
    BIP49,
    BIP84,
}

impl Display for AddressType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BIP44 => write!(f, "BIP44"),
            Self::BIP49 => write!(f, "BIP49"),
            Self::BIP84 => write!(f, "BIP84"),
        }
    }
}
