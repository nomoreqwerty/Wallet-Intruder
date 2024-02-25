use chashmap::CHashMap;
use bitcoin::secp256k1::{All, Secp256k1};
use std::sync::{
    atomic::{AtomicU32, AtomicU64},
    Arc
};
use indicatif::ProgressStyle;
use time_humanize::{Accuracy, Tense};

use crate::{
    common::{
        reusable::{CommonDerivationPaths, TimeTracker},
        *,
    },
    wallet::{AddressType, Wallet}
};

pub struct WalletIntruder {
    thread_pool: threadpool::ThreadPool,
    addresses_map: Option<Arc<CHashMap<String, u64>>>,
    paths: Arc<CommonDerivationPaths>,
    total_checked_wallets: Arc<AtomicU64>,
    wallets_per_second: Arc<AtomicU32>,
    secp: Arc<Secp256k1<All>>,
}

impl WalletIntruder {
    pub fn main() {
        clear_command_line_and_print_logo();

        Self::test_writing_to_file();

        let threads = get_user_threads_value();

        Self::new(threads)
            .read_addresses()
            .pause_for_secs(5)
            .run_stats_displayer(threads)
            .run_wallet_generators(threads)
            .join();
    }

    fn new(cores: usize) -> Self {
        Self {
            thread_pool: threadpool::ThreadPool::new(cores + 1), // + 1 because of the stats displayer
            addresses_map: None,
            paths: Arc::new(CommonDerivationPaths::new()),
            total_checked_wallets: Arc::new(AtomicU64::default()),
            wallets_per_second: Arc::new(AtomicU32::default()),
            secp: Arc::new(Secp256k1::default()),
        }
    }

    fn join(&self) {
        self.thread_pool.join();
    }

    fn run_stats_displayer(self, threads: usize) -> Self {
        clear_command_line_and_print_logo();

        let wallets_per_second = self.wallets_per_second.clone();
        let total_checked_wallets = self.total_checked_wallets.clone();

        self.thread_pool.execute(move || {
            routines::StatsDisplayer {
                total_checked_wallets,
                wallets_per_second,
                threads,
            }
            .run()
        });

        self
    }

    pub fn run_wallet_generators(self, cores: usize) -> Self {
        for _ in 0..cores {
            let addresses_map = self.addresses_map.as_ref().unwrap().clone();
            let paths = self.paths.clone();
            let total_checked_wallets = self.total_checked_wallets.clone();
            let wallets_per_second = self.wallets_per_second.clone();
            let secp = self.secp.clone();

            self.thread_pool.execute(move || {
                routines::WalletGenerator {
                    addresses_map,
                    paths,
                    total_checked_wallets,
                    wallets_per_second,
                    secp,
                }
                .run()
            });
        }

        self
    }

    fn read_addresses(mut self) -> Self {
        leg::info("reading the addresses ... ", None, Some(false));
        let tracker = TimeTracker::start();

        let file_content =
            read_file("./blockchair_bitcoin_addresses_and_balance_LATEST.tsv")
                .expect("Unable to find the `blockchair_bitcoin_addresses_and_balance_LATEST.tsv` file. Download it from http://addresses.loyce.club/ ~ 1.4 GB and put it in the local directory.");

        let tracker = tracker.stop();
        leg::success(
            &format!("done in {:.2}s", tracker.elapsed().as_secs_f32()),
            None,
            None,
        );

        leg::info("Collecting the addresses ... ", None, Some(false));
        let tracker = tracker.restart();

        self.addresses_map = Some(Arc::new(
            parse_addresses(&file_content).expect("can't get addresses"),
        ));

        let tracker = tracker.stop();
        leg::success(
            &format!("done in {:.2}s", tracker.elapsed().as_secs_f32()),
            None,
            None,
        );

        self
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

    fn test_writing_to_file() {
        leg::info("Testing writing to a file ... ", None, Some(false));

        match test_writing_to_file("./writing_test") {
            Ok(_) => leg::success("okay", None, None),
            Err(e) => {
                warn_user_about_writing_error(e);
                if ask_user_for_continue() == UserAnswer::No {
                    panic!("Aborted")
                }
            }
        }
    }
}

mod routines {
    use super::*;
    use colored::Colorize;
    use std::{
        sync::atomic::Ordering,
        time::Duration,
        ops::Deref,
    };

    use crate::defines::YELLOW;

    pub struct StatsDisplayer {
        pub(crate) total_checked_wallets: Arc<AtomicU64>,
        pub(crate) wallets_per_second: Arc<AtomicU32>,
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
                    "{} {} {}\n{} {}\n{} {} w/s\n{} {} wallets",
                    "Using".bright_red(),
                    self.threads,
                    "threads".bright_red(),
                    "Elapsed time:".bright_purple(),
                    //                             This cast is needed to remove nanosecond precision.
                    time_humanize::HumanTime::from(Duration::from_secs(indicator.duration().as_secs())).to_text_en(Accuracy::Precise, Tense::Present),
                    "Speed:".bright_blue(),
                    self.wallets_per_second.load(Ordering::Relaxed),
                    "Total:".custom_color(YELLOW),
                    self.total_checked_wallets.load(Ordering::Relaxed),
                ));

                self.wallets_per_second.store(0, Ordering::Relaxed);
            }
        }
    }
    pub struct WalletGenerator {
        pub(crate) addresses_map: Arc<CHashMap<String, u64>>,
        pub(crate) paths: Arc<CommonDerivationPaths>,
        pub(crate) total_checked_wallets: Arc<AtomicU64>,
        pub(crate) wallets_per_second: Arc<AtomicU32>,
        pub(crate) secp: Arc<Secp256k1<All>>,
    }

    impl WalletGenerator {
        pub fn run(&self) {
            loop {
                let wallet = Wallet::generate(&self.paths, &self.secp);

                if let Some(balance) = self.addresses_map.get(wallet.bip44_addr.as_str()) {
                    append_wallet_to_file(
                        "./found_wallets.txt",
                        &wallet.mnemonic,
                        *balance.deref(),
                    );
                    print_found_wallet(AddressType::BIP44, &wallet, *balance.deref());
                } else if let Some(balance) = self.addresses_map.get(wallet.bip49_addr.as_str()) {
                    append_wallet_to_file(
                        "./found_wallets.txt",
                        &wallet.mnemonic,
                        *balance.deref(),
                    );
                    print_found_wallet(AddressType::BIP49, &wallet, *balance.deref());
                } else if let Some(balance) = self.addresses_map.get(wallet.bip84_addr.as_str()) {
                    append_wallet_to_file(
                        "./found_wallets.txt",
                        &wallet.mnemonic,
                        *balance.deref(),
                    );
                    print_found_wallet(AddressType::BIP84, &wallet, *balance.deref());
                }

                self.total_checked_wallets.fetch_add(1, Ordering::Relaxed);
                self.wallets_per_second.fetch_add(1, Ordering::Relaxed);
            }
        }
    }
}
