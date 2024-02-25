use bip0039::Mnemonic;
use chashmap::CHashMap;
use colored::{Colorize, CustomColor};
use std::{
    io::{Read, Write},
    str::FromStr,
    fs::File
};

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
    }

    impl Default for CommonDerivationPaths {
        fn default() -> Self {
            Self::new()
        }
    }

    impl CommonDerivationPaths {
        pub fn new() -> Self {
            Self {
                bip44: unsafe { DerivationPath::from_str("m/44'/0'/0'/0/0").unwrap_unchecked() },
                bip49: unsafe { DerivationPath::from_str("m/49'/0'/0'/0/0").unwrap_unchecked() },
                bip84: unsafe { DerivationPath::from_str("m/84'/0'/0'/0/0").unwrap_unchecked() },
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

pub fn read_file(path: &str) -> std::io::Result<String> {
    let mut file = File::open(path)?;
    let mut content = String::new();
    file.read_to_string(&mut content)?;
    Ok(content)
}

pub fn parse_addresses(content: &str) -> std::io::Result<CHashMap<String, u64>> {
    let lines = content.split('\n').collect::<Vec<&str>>();

    let map = CHashMap::with_capacity(lines.len());

    for &line in lines[1..].iter() {
        let (addr, balance) = line.split_once('\t').unwrap();
        let balance: u64 = balance.parse().unwrap_or_default();
        map.insert_new(addr.to_owned(), balance);
    }

    Ok(map)
}

/// Append found wallet to a file. If the file does not exist it will be created.
pub fn append_wallet_to_file(path: &str, mnemonic: &Mnemonic, balance: u64) {
    let mut file = File::options().create(true).append(true).open(path)
        .unwrap_or_else(|_| panic!("Unable to open file {path}\nFound wallet:\nmnemonic: {mnemonic}\nbalance: {balance}\n\n\n"));

    file.write_all(format!("mnemonic: {mnemonic}\nbalance: {balance}\n\n\n").as_bytes())
        .unwrap_or_else(|_| panic!("Unable to open file {path}\nFound wallet:\nmnemonic: {mnemonic}\nbalance: {balance}\n\n\n"));
}

pub fn test_writing_to_file(path: &str) -> std::io::Result<()> {
    let path = format!("{path}.test");
    let mut file = File::create(&path)?;
    file.write_all(b"Testing file writing")?;
    std::fs::remove_file(&path)?;
    Ok(())
}

pub fn get_user_threads_value() -> usize {
    let total_cores = sys_info::cpu_num().unwrap();

    print!("how many cores do you want to use? (1-{total_cores})\n> ");
    std::io::stdout().flush().unwrap();

    let mut input = String::new();
    std::io::stdin().read_line(&mut input).unwrap();

    let user_value = input.trim().parse::<u32>().unwrap_or(total_cores);

    let value = user_value.clamp(1, 4) as usize;

    leg::info(&format!("Using {value} cores"), None, None);

    value
}

pub fn print_found_wallet(address_type: AddressType, wallet: &Wallet, balance: u64) {
    println!(
        "Found a wallet:\n{address_type}: {}\nmnemonic: {}\nbalance: {balance}\n\n",
        wallet.bip44_addr, wallet.mnemonic
    );
}

pub fn warn_user_about_writing_error(error: impl std::error::Error) {
    leg::error(&format!("failed with error: {error}"), None, None);
    leg::info(
        "It is likely that you don't have write permissions in the current directory.",
        None,
        None,
    );
    leg::info("It is highly recommended to run the program as an administrator or change the working directory,", None, None);
    leg::info(
        "otherwise if you find a wallet with a balance you might lose it.",
        None,
        None,
    );
}

#[derive(PartialEq, Copy, Clone)]
pub enum UserAnswer {
    Yes,
    No,
}

impl FromStr for UserAnswer {
    type Err = ();

    /// Never fails.
    fn from_str(input: &str) -> Result<Self, Self::Err> {
        match input.trim().to_lowercase().as_str() {
            "y" | "yes" => Ok(UserAnswer::Yes),
            _ => Ok(UserAnswer::No),
        }
    }
}

pub fn ask_user_for_continue() -> UserAnswer {
    println!("Continue? (y/n)\n> ");

    let mut input = String::new();
    std::io::stdin().read_line(&mut input).unwrap();

    UserAnswer::from_str(&input).unwrap()
}

pub fn clear_command_line_and_print_logo() {
    unsafe {
        libc::system("cls\0".as_ptr() as *const i8);
    }

    println!("{}\n", LOGO.custom_color(CustomColor::new(255, 232, 0)));

    // i know it's bad
    const LOGO_LEN: usize = 131;

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
            "https://github.com/nomoreqwerty/Wallet-Intruder".bright_white()
        )
    );
    println!(
        "{: ^LOGO_LEN$}",
        &format!(
            "{} {}",
            "Version:".bright_blue().bold(),
            "0.1.0".bright_white()
        )
    );
    println!();
}
