mod balancer_cmd;
mod catalog_cmd;
mod ctx;
mod groups_cmd;
mod interactive;
mod run_server;
mod service_cmd;
mod settings_cmd;
mod sync_cmd;
mod ui;

use clap::{Parser, Subcommand};
use ctx::AppCtx;

#[derive(Parser)]
#[command(name = "middlewarejson-cli")]
#[command(about = "middlewarejson CLI — каталог, балансировщики, синхронизация")]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Интерактивное меню
    Interactive,
    Settings {
        #[command(subcommand)]
        command: SettingsCommands,
    },
    Service {
        #[command(subcommand)]
        command: ServiceCommands,
    },
    Catalog {
        #[command(subcommand)]
        command: CatalogCommands,
    },
    Group {
        #[command(subcommand)]
        command: GroupCommands,
    },
    Balancer {
        #[command(subcommand)]
        command: BalancerCommands,
    },
}

#[derive(Subcommand)]
enum SettingsCommands {
    Set {
        #[arg(long)]
        panel_token: Option<String>,
        #[arg(long)]
        panel_base_path: Option<String>,
    },
    Show,
    ScriptShow,
    Test,
}

#[derive(Subcommand)]
enum ServiceCommands {
    Status,
    Install,
    Start,
    Restart,
}

#[derive(Subcommand)]
enum CatalogCommands {
    Sync,
    List {
        #[arg(long)]
        active_only: bool,
    },
}

#[derive(Subcommand)]
enum GroupCommands {
    Sync,
    List,
    Assign {
        #[arg(long)]
        group: String,
        #[arg(long)]
        balancer: String,
    },
    Unassign {
        #[arg(long)]
        group: String,
    },
    Show {
        #[arg(long)]
        group: String,
    },
}

#[derive(Subcommand)]
enum BalancerCommands {
    Create {
        #[arg(long)]
        name: String,
        #[arg(long)]
        flag: Option<String>,
        #[arg(long)]
        members: String,
        #[arg(long)]
        tag: Option<String>,
        #[arg(long, default_value = "roundRobin")]
        strategy: String,
        #[arg(long, default_value = "disabled")]
        scope: String,
        #[arg(long, default_value = "")]
        scope_target: String,
    },
    List,
    Configure {
        #[arg(long)]
        tag: String,
    },
    SetScope {
        #[arg(long)]
        tag: String,
        #[arg(long)]
        scope: String,
        #[arg(long, default_value = "")]
        target: String,
    },
    Delete {
        #[arg(long)]
        tag: String,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();
    let cli = Cli::parse();
    let ctx = AppCtx::new();

    match cli.command {
        None | Some(Commands::Interactive) => interactive::run_interactive_menu(&ctx).await?,
        Some(Commands::Settings { command }) => match command {
            SettingsCommands::Set {
                panel_token,
                panel_base_path,
            } => {
                if panel_token.is_none() && panel_base_path.is_none() {
                    anyhow::bail!("Укажите --panel-token и/или --panel-base-path");
                }
                settings_cmd::settings_set(
                    &ctx,
                    panel_token.as_deref(),
                    panel_base_path.as_deref(),
                )?;
            }
            SettingsCommands::Show => settings_cmd::panel_settings_show(&ctx)?,
            SettingsCommands::ScriptShow => settings_cmd::script_settings_show(&ctx)?,
            SettingsCommands::Test => settings_cmd::panel_test(&ctx).await?,
        },
        Some(Commands::Service { command }) => match command {
            ServiceCommands::Status => service_cmd::service_status_menu(&ctx)?,
            ServiceCommands::Install => service_cmd::install_systemd(&ctx)?,
            ServiceCommands::Start => service_cmd::service_start()?,
            ServiceCommands::Restart => service_cmd::service_restart()?,
        },
        Some(Commands::Catalog { command }) => match command {
            CatalogCommands::Sync => catalog_cmd::catalog_sync(&ctx).await?,
            CatalogCommands::List { active_only } => {
                catalog_cmd::catalog_list(&ctx, active_only, false)?;
            }
        },
        Some(Commands::Group { command }) => match command {
            GroupCommands::Sync => groups_cmd::group_sync(&ctx).await?,
            GroupCommands::List => groups_cmd::group_list(&ctx)?,
            GroupCommands::Assign { group, balancer } => {
                groups_cmd::group_assign(&ctx, &group, &balancer)?;
            }
            GroupCommands::Unassign { group } => groups_cmd::group_unassign(&ctx, &group)?,
            GroupCommands::Show { group } => groups_cmd::group_show(&ctx, &group)?,
        },
        Some(Commands::Balancer { command }) => match command {
            BalancerCommands::Create {
                name,
                flag,
                members,
                tag,
                strategy,
                scope,
                scope_target,
            } => {
                let raw_members: Vec<String> = members
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect();
                balancer_cmd::balancer_create(
                    &ctx,
                    &name,
                    flag.as_deref(),
                    &raw_members,
                    tag.as_deref(),
                    &strategy,
                    &scope,
                    &scope_target,
                )?;
            }
            BalancerCommands::List => balancer_cmd::balancer_list(&ctx)?,
            BalancerCommands::Configure { tag } => {
                balancer_cmd::configure_balancer_interactive(&ctx, &tag)?;
            }
            BalancerCommands::SetScope { tag, scope, target } => {
                let repo = ctx.repo()?;
                if !repo.set_balancer_scope(&tag, &scope, &target)? {
                    anyhow::bail!("Балансировщик '{tag}' не найден");
                }
                println!(
                    "{} → {}",
                    tag,
                    middlewarejson_core::models::balancer::format_scope(&scope, &target)
                );
            }
            BalancerCommands::Delete { tag } => {
                let repo = ctx.repo()?;
                if repo.delete_balancer(&tag)? {
                    println!("Удалён балансировщик '{tag}'");
                } else {
                    anyhow::bail!("Балансировщик '{tag}' не найден");
                }
            }
        },
    }
    Ok(())
}