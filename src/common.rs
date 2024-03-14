use bip0039::Mnemonic;
use colored::{Colorize, CustomColor};
use std::{
    io::{Read, Write},
    str::FromStr,
    fs::File
};
use std::convert::Infallible;
use std::path::Path;
use hand::*;
use hashbrown::HashMap;

use thiserror::Error;

use crate::{
    defines::LOGO,
    wallet::*
};

pub mod reusable {
    use bitcoin::bip32::DerivationPath;
    use std::str::FromStr;
    use std::time::{Duration, Instant};

    pub struct CommonDerivationPaths {
        pub bip44: DerivationPath,
        pub bip49: DerivationPath,
        pub bip84: DerivationPath,
        pub bip86: DerivationPath,
        pub bip141:  DerivationPath,
    }

    impl Default for CommonDerivationPaths {
        fn default() -> Self {
            Self::new()
        }
    }

    impl CommonDerivationPaths {
        pub fn new() -> Self {
            Self {
                // these paths are valid so they won't fall
                bip44: DerivationPath::from_str("m/44'/0'/0'/0/0").unwrap(),
                bip49: DerivationPath::from_str("m/49'/0'/0'/0/0").unwrap(),
                bip84: DerivationPath::from_str("m/84'/0'/0'/0/0").unwrap(),
                bip86: DerivationPath::from_str("m/86'/0'/0'/0/0").unwrap(),
                bip141:  DerivationPath::from_str("m/0").unwrap(),
            }
        }
    }

    pub struct Waiting;
    pub struct Stopped;

    pub struct TimeTracker<State = Waiting> {
        start: Instant,
        elapsed: Duration,
        state: std::marker::PhantomData<State>,
    }

    impl TimeTracker {
        pub fn start() -> TimeTracker<Waiting> {
            TimeTracker {
                start: Instant::now(),
                elapsed: Duration::default(),
                state: std::marker::PhantomData::<Waiting>,
            }
        }
    }

    impl TimeTracker<Waiting> {
        pub fn stop(self) -> TimeTracker<Stopped> {
            TimeTracker {
                start: self.start,
                elapsed: (Instant::now() - self.start),
                state: std::marker::PhantomData::<Stopped>,
            }
        }
    }

    impl TimeTracker<Stopped> {
        pub fn elapsed(&self) -> &Duration {
            &self.elapsed
        }

        pub fn restart(self) -> TimeTracker<Waiting> {
            TimeTracker::start()
        }
    }
}

pub fn read_file(path: &Path) -> std::io::Result<String> {
    let mut file = File::open(path)?;
    let mut content = String::new();
    file.read_to_string(&mut content)?;
    Ok(content)
}

pub fn parse_addresses(content: &str) -> Result<HashMap<String, u64>, ParseAddressesError> {
    let lines = content.split('\n').collect::<Vec<&str>>();

    let mut map = HashMap::with_capacity(lines.len());

    if lines[0] != "address\tbalance" && lines[0] != "address balance" {
        return Err(ParseAddressesError::InvalidHeader { expected: "address balance".into(), found: lines[0].into() });
    }

    for &line in lines[1..].iter() {
        let (addr, balance) = line.split_once('\t')
            .ok_or(ParseAddressesError::SplitError { separator: '\t'.into(), line: line.into() })?;

        let mut starts_with: Option<&str> = None;

        // filter addresses block start //

        // checks if address starts with bc1q, bc1q, 1 or 3 and saves the prefix
        for &prefix in ["bc1q", "bc1p", "1", "3"].iter() {
            if addr.starts_with(prefix) {
                starts_with = Some(prefix);
                break;
            }
        }

        // if the address does not start with any of the prefixes, continue
        // also check for the p2wsh address (it is no implemented yet) and skip it
        if let Some(prefix) = starts_with {
            if prefix == "bc1q" && addr.len() > 42 { continue }
        } else { continue }

        // filter addresses block end //

        let balance: u64 = balance.parse().unwrap_or_default();

        map.insert(addr.to_owned(), balance);
    }

    Ok(map)
}

#[derive(Debug, Error)]
pub enum ParseAddressesError {
    #[error("unable to split address and balance by {separator:?}, got {line:?}")]
    SplitError {
        separator: String,
        line: String,
    },

    #[error("invalid header. first line of the .tsv file is expected to be {expected:?}, got {found:?}")]
    InvalidHeader {
        expected: String,
        found: String,
    }
}

/// Append found wallet to a file. If the file does not exist it will be created.
pub fn append_wallet_to_file(path: &Path, mnemonic: &Mnemonic, balance: u64) -> Result<(), AppendWalletError> {
    let mut file = File::options().create(true).append(true).open(path)
        .map_err(|error| AppendWalletError::OpeningFileError { file: path.file_name().unwrap_or_default().to_str().unwrap_or("none").into(), error })?;

    file.write_all(format!("mnemonic: {mnemonic}\nbalance: {balance}\n\n\n").as_bytes())
        .map_err(|error| AppendWalletError::WritingToFileError { file: path.to_str().unwrap_or("none").into(), error })?;

    Ok(())
}

#[derive(Debug, Error)]
pub enum AppendWalletError {
    #[error("unable to open `{file}`")]
    OpeningFileError {
        file: String,
        error: std::io::Error,
    },

    #[error("unable to save a wallet to `{file}`")]
    WritingToFileError {
        file: String,
        error: std::io::Error,
    }
}

pub fn test_writing_to_file(path: &str) -> std::io::Result<()> {
    let path = format!("{path}.test");
    let mut file = File::create(&path)?;
    file.write_all(b"Testing file writing")?;
    std::fs::remove_file(&path)?;
    Ok(())
}

pub fn ask_user_threads_amount() -> Result<usize, AskUserThreadsAmountError> {
    let total_cores = sys_info::cpu_num()
        .map_err(AskUserThreadsAmountError::GetCoresCountError)?;

    tracing::debug!("total cores amount = {total_cores}");

    print!("how many cores do you want to use? (1-{total_cores})\n> ");
    std::io::stdout().flush()
        .map_err(AskUserThreadsAmountError::IOError)?;

    let mut input = String::new();

    std::io::stdin().read_line(&mut input)
        .map_err(AskUserThreadsAmountError::IOError)?;

    let user_value = input.trim().parse::<u32>().unwrap_or(total_cores);

    let value = user_value.clamp(1, total_cores) as usize;

    tracing::info!("utilizing {value}/{total_cores} cores");

    infoln!("Using {} cores", value);

    Ok(value)
}

#[derive(Debug, Error)]
pub enum AskUserThreadsAmountError {
    #[error("io error: {0}")]
    IOError(#[from] std::io::Error),

    #[error("failed to get number of cores: {0}")]
    GetCoresCountError(#[from] sys_info::Error),
}

pub fn print_found_wallet(address_type: AddressType, wallet: &Wallet, balance: u64) {
    clear_command_line_and_print_logo();

    successln!(
        "Found a wallet\n{address_type}: {}\nmnemonic: {}\nbalance: {balance}\n\n",
        wallet.p2pkh_addr, wallet.mnemonic
    );
}

pub fn warn_user_about_writing_error() {
    warnln!("It is likely that you don't have write permissions to write in the current directory.");
    warnln!("It is highly recommended to run the program as an administrator or change the working directory,");
    warnln!("otherwise, if you find a wallet with a balance you might lose it.");
}

#[derive(PartialEq, Copy, Clone)]
pub enum UserAnswer { Yes, No }

impl FromStr for UserAnswer {
    type Err = Infallible;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        match input.trim().to_lowercase().as_str() {
            "y" | "yes" => Ok(UserAnswer::Yes),
            _ => Ok(UserAnswer::No),
        }
    }
}

pub fn ask_user_for_continue() -> Result<UserAnswer, std::io::Error> {
    println!("Continue? (y/n)\n> ");

    let mut input = String::new();
    std::io::stdin().read_line(&mut input)?;

    Ok(UserAnswer::from_str(&input).unwrap())
}

pub fn clear_command_line_and_print_logo() {
    // just clearing the terminal, should not fall
    #[cfg(target_os = "windows")]
    unsafe {
        libc::system("cls\0".as_ptr() as *const i8);
    }

    #[cfg(target_os = "linux")]
    unsafe {
        libc::system("clear\0".as_ptr() as *const i8);
    }

    println!("{}\n", LOGO.custom_color(CustomColor::new(255, 232, 0)));

    const LOGO_LEN: usize = 131; // hardcode

    println!(
        "{: ^LOGO_LEN$}",
        &format!(
            "{} {}",
            "Author:".bright_blue().bold(),
            "nomoreqwerty".bright_white()
        )
    );
    println!(
        "{: ^LOGO_LEN$}",
        &format!(
            "{} {}",
            "GitHub:".bright_blue().bold(),
            "github.com/nomoreqwerty/Wallet-Intruder".bright_white()
        )
    );
    println!(
        "{: ^LOGO_LEN$}",
        &format!(
            "{} {}",
            "Version:".bright_blue().bold(),
            "0.2.0".bright_white()
        )
    );
    println!();
}
