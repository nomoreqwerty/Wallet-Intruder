

use std::path::Path;
use std::process::exit;
use hashbrown::HashMap;
use bitcoin::secp256k1::{All, Secp256k1};
use std::sync::{
    atomic::{AtomicU32, AtomicU64},
    Arc
};
use indicatif::ProgressStyle;
use time_humanize::{Accuracy, Tense};
use hand::*;
use path_absolutize::Absolutize;
use thiserror::Error;


use crate::{
    common::{
        reusable::{CommonDerivationPaths, TimeTracker},
        *,
    },
    wallet::{AddressType, Wallet}
};
use crate::defines::TRACING_LEVEL;

pub struct WalletIntruder {
    thread_pool: threadpool::ThreadPool,
    addresses_map: Option<Arc<HashMap<String, u64>>>,
    paths: Arc<CommonDerivationPaths>,
    matched_wallets: Arc<AtomicU32>,
    generated_wallets: Arc<AtomicU64>,
    wallets_per_second: Arc<AtomicU32>,
    secp: Arc<Secp256k1<All>>,
}

impl WalletIntruder {
    pub fn main() -> anyhow::Result<()> {
        tracing_subscriber::fmt()
            .with_max_level(TRACING_LEVEL)
            .init();

        clear_command_line_and_print_logo();

        let addresses_file = Path::new("./blockchair_bitcoin_addresses_and_balance_LATEST.tsv")
            .absolutize()?.to_path_buf();

        Self::check_addresses_file_exists(addresses_file.as_path())?;

        Self::test_writing_to_file()?;

        let threads = ask_user_threads_amount()?;

        Self::new(threads)
            .read_addresses(addresses_file.as_path())?
            .pause_for_secs(5)
            .run_stats_displayer(threads)
            .run_wallet_generators(threads)?
            .join();

        Ok(())
    }

    fn new(cores: usize) -> Self {
        Self {
            thread_pool: threadpool::ThreadPool::new(cores + 1), // + 1 because of the stats displayer
            addresses_map: None,
            paths: Arc::new(CommonDerivationPaths::new()),
            matched_wallets: Arc::new(AtomicU32::default()),
            generated_wallets: Arc::new(AtomicU64::default()),
            wallets_per_second: Arc::new(AtomicU32::default()),
            secp: Arc::new(Secp256k1::default()),
        }
    }

    fn join(&self) {
        self.thread_pool.join();
    }

    fn check_addresses_file_exists(file: &Path) -> Result<(), CheckAddressesFileExist> {
        if !file.exists() { return Err(CheckAddressesFileExist::AddressesFileDoesNotExist(file.display().to_string())) }
        Ok(())
    }

    fn run_stats_displayer(self, threads: usize) -> Self {
        clear_command_line_and_print_logo();

        let wallets_per_second = self.wallets_per_second.clone();
        let total_checked_wallets = self.generated_wallets.clone();
        let matched_wallets = self.matched_wallets.clone();

        self.thread_pool.execute(move || {
            routines::StatsDisplayer {
                total_checked_wallets,
                wallets_per_second,
                matched_wallets,
                threads,
            }
            .run()
        });

        self
    }

    pub fn run_wallet_generators(self, cores: usize) -> Result<Self, GeneratorError> {
        for id in 0..cores {
            let addresses_map = self.addresses_map.as_ref().unwrap().clone();
            let paths = self.paths.clone();
            let matched_wallets = self.matched_wallets.clone();
            let generated_wallets = self.generated_wallets.clone();
            let wallets_per_second = self.wallets_per_second.clone();
            let secp = self.secp.clone();

            self.thread_pool.execute(move || {
                let generator = routines::WalletGenerator::new(
                    addresses_map, paths, matched_wallets, generated_wallets, wallets_per_second, secp
                );

                if let Err(error) = generator.run(id as u32) {
                    tracing::error!("`wallet generator {id}` has failed with error: {error}");
                }
            });
        }

        Ok(self)
    }

    fn read_addresses(mut self, file: &Path) -> Result<Self, ReadAddressesError> {
        let scope = tracing::info_span!("read_addresses");

        scope.in_scope(|| tracing::trace!("started reading the addresses"));

        info!("reading the addresses ... ");

        let tracker = TimeTracker::start();

        let file_content = read_file(file)
            .map_err(|error| ReadAddressesError::ReadingFileError { file: file.to_str().unwrap_or("none").into(), error })?;

        let tracker = tracker.stop();

        scope.in_scope(|| tracing::info!("have read addresses in {:.2} secs", tracker.elapsed().as_secs_f32()));

        successln!("done in {:.2}s", tracker.elapsed().as_secs_f32());

        scope.in_scope(|| tracing::trace!("started collecting addresses"));

        info!("Collecting the addresses ... ");
        let tracker = tracker.restart();

        self.addresses_map = Some(Arc::new(
            parse_addresses(file_content.trim())
                .map_err(ReadAddressesError::ParsingAddressesError)?
        ));

        let tracker = tracker.stop();

        successln!("done in {:.2}s", tracker.elapsed().as_secs_f32());

        Ok(self)
    }

    fn pause_for_secs(self, secs: u64) -> Self {
        let indicator = indicatif::ProgressBar::new_spinner();
        indicator.set_style(
            ProgressStyle::with_template("{msg}").unwrap()
        );

        for i in (1..=secs).rev() {
            let ui = i as usize;
            indicator.set_message(format!(
                "Continuing in {}{:.<ui$}",
                time_humanize::HumanTime::from(std::time::Duration::from_secs(i)).to_text_en(Accuracy::Precise, Tense::Present),
                "",
            ));

            std::thread::sleep(std::time::Duration::from_secs(1));
        }

        self
    }

    fn test_writing_to_file() -> Result<(), std::io::Error> {
        let file = "./writing_test";

        match test_writing_to_file(file) {
            Ok(_) => tracing::info!("successfully tested writing to file"),
            Err(error) => {
                tracing::error!("{error}");

                warn_user_about_writing_error();

                if ask_user_for_continue()? == UserAnswer::No {
                    exit(0);
                }
            }
        }

        Ok(())
    }
}

#[derive(Debug, Error)]
pub enum CheckAddressesFileExist {
    #[error("`{0}` (bitcoin addresses and balances file) does not exist")]
    AddressesFileDoesNotExist(String)
}

mod routines {
    use super::*;
    use colored::Colorize;
    use std::{
        sync::atomic::Ordering,
        time::Duration,
    };
    

    use crate::defines::YELLOW;

    pub struct StatsDisplayer {
        pub(crate) total_checked_wallets: Arc<AtomicU64>,
        pub(crate) wallets_per_second: Arc<AtomicU32>,
        pub(crate) matched_wallets: Arc<AtomicU32>,
        pub(crate) threads: usize,
    }

    impl StatsDisplayer {
        pub fn run(&self) {
            let indicator = indicatif::ProgressBar::new(100);

            indicator.set_style(
                ProgressStyle::with_template(&format!(
                    "\n{0} {1}\n{2}",
                    "####".bright_white().bold(),
                    "Statistics".custom_color(YELLOW).bold(),
                    "{msg}",
                ))
                .unwrap(),
            );

            loop {
                std::thread::sleep(Duration::from_secs(1));

                indicator.set_message(format!(
                    "{} {} {}\n{} {}\n{} {} w/s\n{} {} wallets\n{} {} wallets",
                    "Using".bright_red(),
                    self.threads,
                    "threads".bright_red(),
                    "Elapsed time:".bright_purple(),
                    //                             This cast is needed to remove nanosecond precision.
                    time_humanize::HumanTime::from(Duration::from_secs(indicator.duration().as_secs())).to_text_en(Accuracy::Precise, Tense::Present),
                    "Speed:".bright_blue(),
                    self.wallets_per_second.load(Ordering::Relaxed),
                    "Found:".bright_green(),
                    self.matched_wallets.load(Ordering::Relaxed),
                    "Generated:".custom_color(YELLOW),
                    self.total_checked_wallets.load(Ordering::Relaxed),
                ));

                self.wallets_per_second.store(0, Ordering::Relaxed);
            }
        }
    }
    pub struct WalletGenerator {
        pub(crate) addresses_map: Arc<HashMap<String, u64>>,
        pub(crate) paths: Arc<CommonDerivationPaths>,
        pub(crate) matched_wallets: Arc<AtomicU32>,
        pub(crate) generated_wallets: Arc<AtomicU64>,
        pub(crate) wallets_per_second: Arc<AtomicU32>,
        pub(crate) secp: Arc<Secp256k1<All>>,
    }

    impl WalletGenerator {
        pub fn new(
            addresses_map: Arc<HashMap<String, u64>>,
            paths: Arc<CommonDerivationPaths>,
            matched_wallets: Arc<AtomicU32>,
            generated_wallets: Arc<AtomicU64>,
            wallets_per_second: Arc<AtomicU32>,
            secp: Arc<Secp256k1<All>>,
        ) -> Self {
            Self {
                addresses_map,
                paths,
                matched_wallets,
                generated_wallets,
                wallets_per_second,
                secp,
            }
        }

        pub fn run(&self, thread_id: impl Into<Option<u32>>) -> Result<(), GeneratorError> {
            let id = thread_id.into().map_or(String::new(), |id| id.to_string());

            let scope = tracing::trace_span!("wallet generator ", id);
            let _enter = scope.enter();

            let file_save_path = "./found_wallets.txt";
            let abs_file_path = Path::new(file_save_path)
                .absolutize()
                .map_err(|error| GeneratorError::PathAbsoluteizeError {
                    path: file_save_path.into(),
                    error
                })?;
            let file = abs_file_path.to_path_buf();

            tracing::info!("saving found wallets to `{}`", file.display());

            tracing::info!("start generating");

            loop {
                let wallet = Wallet::generate(&self.paths, &self.secp)
                    .map_err(GeneratorError::WalletGeneratingError)?;

                if let Some(balance) = self.addresses_map.get(wallet.p2pkh_addr.as_str()) {
                    self.process_wallet(file.as_path(), &wallet, *balance, AddressType::BIP44)
                        .map_err(GeneratorError::WalletProcessingError)?;
                } else if let Some(balance) = self.addresses_map.get(wallet.p2shwpkh_addr.as_str()) {
                    self.process_wallet(file.as_path(), &wallet, *balance, AddressType::BIP49)
                        .map_err(GeneratorError::WalletProcessingError)?;
                } else if let Some(balance) = self.addresses_map.get(wallet.p2wpkh_addr.as_str()) {
                    self.process_wallet(file.as_path(), &wallet, *balance, AddressType::BIP84)
                        .map_err(GeneratorError::WalletProcessingError)?;
                }

                self.update_counters();
            }
        }

        fn process_wallet(&self, file: &Path, wallet: &Wallet, balance: u64, address_type: AddressType) -> Result<(), WalletProcessingError> {
            tracing::debug!("processing wallet {wallet:?} with balance {balance}");

            append_wallet_to_file(file, &wallet.mnemonic, balance)
                .map_err(|error| WalletProcessingError::SavingWalletToFileError {
                    wallet: Box::new(wallet.clone()),
                    file: file.to_str().unwrap().to_string(),
                    error
                })?;

            print_found_wallet(address_type, wallet, balance);

            self.matched_wallets.fetch_add(1, Ordering::Relaxed);

            Ok(())
        }

        fn update_counters(&self) {
            self.generated_wallets.fetch_add(1, Ordering::Relaxed);
            self.wallets_per_second.fetch_add(1, Ordering::Relaxed);
        }
    }
}

#[derive(Debug, Error)]
pub enum WalletProcessingError {
    #[error("failed to append a wallet to a file `{file}`: {error}")]
    SavingWalletToFileError {
        wallet: Box<Wallet>,
        file: String,
        error: AppendWalletError,
    },
}

#[derive(Debug, Error)]
pub enum GeneratorError {
    #[error("failed `{path}` path absolution: {error}")]
    PathAbsoluteizeError{
        path: String,
        error: std::io::Error,
    },

    #[error("failed to process a wallet: {0}")]
    WalletProcessingError(#[from] WalletProcessingError),

    #[error("failed to generate a wallet:")]
    WalletGeneratingError(#[from] bitcoin::bip32::Error),
}

#[derive(Debug, Error)]
pub enum ReadAddressesError {
    #[error("reading `{file}` error: {error}")]
    ReadingFileError {
        file: String,
        error: std::io::Error,
    },

    #[error("`{path}` absolutizing error: {error}")]
    PathAbsolutionError {
        path: String,
        error: std::io::Error
    },

    #[error("parsing addresses and balances error: {0}")]
    ParsingAddressesError(#[from] ParseAddressesError),
}