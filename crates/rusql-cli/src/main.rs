//! rusql administration CLI.

use clap::Parser;

#[derive(Debug, Parser)]
#[command(name = "rusql-cli", version, about)]
struct Args {
    /// Locale (en-US or zh-CN)
    #[arg(long, env = "RUSQL_LOCALE", default_value = "en-US")]
    locale: String,
}

fn main() {
    let args = Args::parse();
    if std::env::var("RUSQL_LOCALE").is_err() {
        rusql_i18n::set_locale(&args.locale);
    }
    rusql_i18n::init();
    println!("{}", rusql_i18n::messages::cli_usage());
}
