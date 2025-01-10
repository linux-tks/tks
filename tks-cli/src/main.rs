mod import_kwallet;

use anyhow::Result;
use clap::{Parser, Subcommand};
use clap_verbosity_flag::{InfoLevel, Verbosity};
use colored::Colorize;
use console::Term;
use std::io::Read;
use std::{io, process::exit};
use yubikey::{Context, Key, Serial, YubiKey};
use yubikey::piv::SlotId;
use import_kwallet::ImportKwalletCmd;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[command(subcommand)]
    cmd: Commands,

    /// Run the tool in verbose mode
    #[command(flatten)]
    verbosity: Verbosity<InfoLevel>,
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
struct ImportGnomeCmd {}
#[derive(Parser, Debug)]
struct ImportPassCmd {}

#[derive(Subcommand, Debug)]
enum ImportCmd {
    #[clap(verbatim_doc_comment)]
    /// This command imports an XML file obtained by using KWalletManager's "export as XML" feature
    ///
    /// The KWallet data is typically organized in several main folders. The well known default
    /// folder is the `Passwords` folder. Another default folder name is `FormData`. Then, we can have
    /// any other arbitrary folders at the top of the wallet. The name of the original folder is
    /// being put into a special attribute attached to each item. This attributes name is
    /// `tks:kwallet-folder`.
    ///
    /// NOTE: Currently, there is no known mapping between KWallet Map entries and Secret Service
    /// items. For this reason, this tool ignores the Map entries. Same applies to FormData. If you
    /// happen to know how to map these from KWallet to Secret Service, then please issue a Pull Request.
    ///
    /// KWallet entry type can be passwords, maps, binary data or unknown. We use the attribute
    /// `tks:kwallet-entry-type` to store the initial item type.
    ///
    /// In addition to above attribute, each item will also receive the following attributes:
    /// `xdg:schema`:`org.freedesktop.Secret.Generic'
    /// `xdg:creator`:`org.kde.KWallet`
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

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    let args = Args::parse();

    pretty_env_logger::formatted_builder().filter_level(args.verbosity.into()).init();

    match args.cmd {
        Commands::Yk { yk_cmd } => yk_cmd.run(),
        Commands::Service { service_cmd } => service_cmd.run(),
        Commands::Import { import_cmd } => import_cmd.run().await?,
    }
    Ok(())
}

impl YkCmd {
    fn run(&self) {
        match self {
            YkCmd::Enroll(enroll) => enroll.run(),
            YkCmd::List(list) => list.run(),
        }
        .unwrap_or_else(|e| {
            log::debug!("Error: {:?}", e);
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
    async fn run(&self) -> Result<()> {
        match self {
            ImportCmd::Kwallet(cmd) => cmd.run().await,
            ImportCmd::Gnome(cmd) => cmd.run().await,
            ImportCmd::Pass(cmd) => cmd.run().await,
        }
    }
}
impl ImportGnomeCmd {
    async fn run(&self) -> Result<()> {
        todo!()
    }
}
impl ImportPassCmd {
    async fn run(&self) -> Result<()> {
        todo!()
    }
}
