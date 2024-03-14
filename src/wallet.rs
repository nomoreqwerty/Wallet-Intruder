use bip0039::{Count, English, Mnemonic};
use bitcoin::{secp256k1::{All, Secp256k1}, Network};
use std::{
    fmt::Display,
    sync::Arc,
};
use bitcoin::key::UntweakedPublicKey;
use thiserror::Error;

use crate::common::reusable::CommonDerivationPaths;

#[derive(Debug, Error)]
pub enum GenerateWalletError {

}

#[derive(Debug, Clone)]
pub struct Wallet {
    pub mnemonic: Mnemonic,
    pub p2pkh_addr: String,
    pub p2shwpkh_addr: String,
    pub p2wpkh_addr: String,
    pub p2tr_addr: String,
}

impl Wallet {
    pub fn generate(paths: &Arc<CommonDerivationPaths>, secp: &Arc<Secp256k1<All>>) -> Result<Self, bitcoin::bip32::Error> {
        let mnemonic: Mnemonic<English> = Mnemonic::generate(Count::Words12);

        let xprv = bitcoin::bip32::Xpriv::new_master(Network::Bitcoin, &mnemonic.to_seed(""))?;

        let bip44_xprv  = xprv.derive_priv(secp, &paths.bip44).unwrap();    //
        let bip49_xprv  = xprv.derive_priv(secp, &paths.bip49).unwrap();    // there is no Err returned in this function, but it's Result<_, _> ???
        let bip84_xprv  = xprv.derive_priv(secp, &paths.bip84).unwrap();    //
        let bip86_xprv  = xprv.derive_priv(secp, &paths.bip86).unwrap();    //

        let arc_secp = secp.clone();

        let p2pkh_addr = bitcoin::Address::p2pkh(
            &bip44_xprv.to_priv().public_key(&arc_secp),
            Network::Bitcoin,
        );

        let p2shwpkh_addr = bitcoin::Address::p2shwpkh(
            &bip49_xprv.to_priv().public_key(&arc_secp),
            Network::Bitcoin,
        ).unwrap();

        let p2wpkh_addr = bitcoin::Address::p2wpkh(
            &bip84_xprv.to_priv().public_key(&arc_secp),
            Network::Bitcoin,
        ).unwrap();

        let p2tr_addr = bitcoin::Address::p2tr(
            secp,
            UntweakedPublicKey::from(bip86_xprv.to_priv().public_key(&arc_secp)),
            None,
            Network::Bitcoin
        );

        Ok(Self {
            mnemonic,
            p2pkh_addr: p2pkh_addr.to_string(),
            p2shwpkh_addr: p2shwpkh_addr.to_string(),
            p2wpkh_addr: p2wpkh_addr.to_string(),
            p2tr_addr: p2tr_addr.to_string(),
        })
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
