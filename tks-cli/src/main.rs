use clap::{Parser, Subcommand};
extern crate pretty_env_logger;
#[macro_use]
extern crate log;
use colored::Colorize;
use console::Term;
use std::io::Read;
use std::{io, process::exit};
use yubikey::{Context, Key, Serial, YubiKey};
use yubikey::piv::SlotId;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[command(subcommand)]
    cmd: Commands,

    /// Verbose mode
    #[arg(short, long)]
    verbose: bool,
}

#[derive(Parser, Debug)]
struct YkEnrollCmd {}
#[derive(Parser, Debug)]
struct YkListCmd {}

#[derive(Subcommand, Debug)]
enum YkCmd {
    /// List connected Yubikeys
    List(YkListCmd),
    /// Enroll Yubikeys with tks-service
    Enroll(YkEnrollCmd),
}

#[derive(Parser, Debug)]
struct ServiceStatusCmd {}

#[derive(Subcommand, Debug)]
enum ServiceCmd {
    /// Display information about the service
    Status(ServiceStatusCmd),
}

#[derive(Parser, Debug)]
struct ImportKwalletCmd {}
#[derive(Parser, Debug)]
struct ImportGnomeCmd {}
#[derive(Parser, Debug)]
struct ImportPassCmd {}

#[derive(Subcommand, Debug)]
enum ImportCmd {
    /// Import from KWallet
    Kwallet(ImportKwalletCmd),
    /// Import from GNOME Keyring
    Gnome(ImportGnomeCmd),
    /// Import from PASS
    Pass(ImportPassCmd),
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Yuibkey-related commands
    Yk {
        #[command(subcommand)]
        yk_cmd: YkCmd,
    },
    /// Service-related commands
    Service {
        #[command(subcommand)]
        service_cmd: ServiceCmd,
    },
    /// Import operations
    Import {
        #[command(subcommand)]
        import_cmd: ImportCmd,
    },
}

fn main() {
    let args = Args::parse();

    if args.verbose {
        // When debugging, developers can set the TKS_CLI_LOG environment variable to control the log level prior to running the program
        if let Err(_) = std::env::var("TKS_CLI_LOG") {
            std::env::set_var("TKS_CLI_LOG", "info");
        }
    }
    pretty_env_logger::init_custom_env("TKS_CLI_LOG");

    match args.cmd {
        Commands::Yk { yk_cmd } => yk_cmd.run(),
        Commands::Service { service_cmd } => service_cmd.run(),
        Commands::Import { import_cmd } => import_cmd.run(),
    }
}

impl YkCmd {
    fn run(&self) {
        match self {
            YkCmd::Enroll(enroll) => enroll.run(),
            YkCmd::List(list) => list.run(),
        }
        .unwrap_or_else(|e| {
            debug!("Error: {:?}", e);
            e.print();
        })
    }
}

type CliResult<T> = Result<T, CliError>;

#[derive(Debug)]
enum CliError {
    YubikeyError(yubikey::Error),
    Cancelled,
    IoError(std::io::Error),
}

impl CliError {
    pub(crate) fn print(&self) {
        match self {
            CliError::IoError(e) => println!("IO Error"),
            CliError::YubikeyError(e) => println!("Yubikey access error"),
            CliError::Cancelled => println!("Operation cancelled by the user"),
        }
    }
}

impl From<yubikey::Error> for CliError {
    fn from(err: yubikey::Error) -> Self {
        CliError::YubikeyError(err)
    }
}
impl From<std::io::Error> for CliError {
    fn from(value: io::Error) -> Self {
        CliError::IoError(value)
    }
}
impl YkEnrollCmd {
    fn run(&self) -> CliResult<()> {
        println!("{}", "Enrolling YubiKeys".bold());
        print!("  Checking for internet connection... ");
        if let Ok(_) = reqwest::blocking::get("https://www.google.com") {
            println!("{}", "CONNECTED TO INTERNET".red());
            println!(
                "  {}: Performing enrollment while connected to the internet",
                "WARNING".bold()
            )
        } else {
            println!("NO CONNECTION DETECTED");
        }
        println!("  Insert Yubikey and press Enter");
        let _ = io::stdin().read_line(&mut String::new()).unwrap();
        let mut yubikey = YubiKey::open()?;
        let keys = Key::list(&mut yubikey)?
            .iter()
            .filter(|k| k.slot() == SlotId::Authentication)
            .count();
        if keys > 0 {
            println!(
                "  {}: Key {} already contains an Authentication certificate. Overwrite? (y/N)",
                "WARNING".bold(),
                yubikey.serial()
            );
            let term = Term::stdout();
            let choice = term.read_char()?;
            match choice {
                'y' | 'Y' => println!("  Overwriting key {}", yubikey.serial().to_string().bold()),
                _ => return Err(CliError::Cancelled),
            }
        } else {
            println!("  {}", "Key is empty.".green());
        }
        println!(
            "  Provisioning key {}...",
            yubikey.serial().to_string().bold()
        );
        Ok(())
    }
}
impl YkListCmd {
    fn run(&self) -> CliResult<()> {
        println!("Searching for connected Yubikeys...");

        let mut readers = Context::open()?;
        let readers_iter = readers.iter()?;

        if readers_iter.len() == 0 {
            println!("No Yubikeys detected");
            return Ok(());
        }

        for (i, reader) in readers_iter.enumerate() {
            let name = reader.name();
            if let Ok(yubikey) = reader.open() {
                let serial = yubikey.serial();
                println!(
                    "{}: {} (serial: {})",
                    (i + 1).to_string().bold(),
                    name,
                    serial
                );
            };
        }
        Ok(())
    }
}
impl ServiceCmd {
    fn run(&self) {
        match self {
            ServiceCmd::Status(cmd) => cmd.run(),
        }
    }
}
impl ServiceStatusCmd {
    fn run(&self) {
        println!("Not yet implemented.");
    }
}
impl ImportCmd {
    fn run(&self) {
        match self {
            ImportCmd::Kwallet(cmd) => cmd.run(),
            ImportCmd::Gnome(cmd) => cmd.run(),
            ImportCmd::Pass(cmd) => cmd.run(),
        }
    }
}
impl ImportKwalletCmd {
    fn run(&self) {
        println!("Not yet implemented.")
    }
}
impl ImportGnomeCmd {
    fn run(&self) {
        println!("Not yet implemented.")
    }
}
impl ImportPassCmd {
    fn run(&self) {
        println!("Not yet implemented.")
    }
}
