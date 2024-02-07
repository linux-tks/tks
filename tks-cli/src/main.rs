use clap::{Parser, Subcommand};
extern crate pretty_env_logger;
#[macro_use]
extern crate log;
use colored::Colorize;
use std::process::exit;
use yubikey::{Context, Serial};

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
    }
}
impl YkEnrollCmd {
    fn run(&self) {
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
    }
}
impl YkListCmd {
    fn run(&self) {
        println!("Searching for connected Yubikeys...");

        let mut readers = Context::open().unwrap_or_else(|e| {
            debug!("couldn't open PC/SC context: {}", e);
            println!("Cannot open PC/SC daemon");
            exit(1);
        });

        let readers_iter = readers.iter().unwrap_or_else(|e| {
            debug!("couldn't enumerate PC/SC readers: {}", e);
            println!("Cannot enumerate PC/SC daemon");
            exit(1);
        });

        if readers_iter.len() == 0 {
            println!("No Yubikeys detected");
            exit(1);
        }

        for (i, reader) in readers_iter.enumerate() {
            let name = reader.name();
            let yubikey = match reader.open() {
                Ok(yk) => yk,
                Err(_) => continue,
            };

            let serial = yubikey.serial();
            println!(
                "{}: {} (serial: {})",
                (i + 1).to_string().bold(),
                name,
                serial
            );
        }
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
