use lsp_server::Notification;
use serde_json::json;
use server::{args::Cli, cli_backend::CliBackend, server::Server};
use clap::Parser;
use tracing::{info, Level, error};
use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_panic::panic_hook;
use tracing_subscriber::{fmt, FmtSubscriber, layer::SubscriberExt};
use server::core::odoo::Odoo;
use std::env;
use std::sync::Arc;

fn main() {
    env::set_var("RUST_BACKTRACE", "full");
    let cli = Cli::parse();
    let use_debug = cli.use_tcp;

    let file_appender = RollingFileAppender::builder()
        .max_log_files(5) // only the most recent 5 log files will be kept
        .rotation(Rotation::HOURLY)
        .filename_prefix(format!("odoo_logs_{}.log", std::process::id()))
        .build("./logs")
        .expect("failed to initialize rolling file appender");
    let (file_writer, _guard) = tracing_appender::non_blocking(file_appender);
    let subscriber = FmtSubscriber::builder()
        .with_thread_ids(true)
        .with_file(false)
        .with_max_level(Level::TRACE)
        .with_ansi(false)
        .with_writer(file_writer)
        .finish();
    if cli.parse || use_debug {
        let stdout_subscriber = fmt::layer().with_writer(std::io::stdout).with_ansi(true);
        tracing::subscriber::set_global_default(subscriber.with(stdout_subscriber)).expect("Unable to set default tracing subscriber");
    } else {
        tracing::subscriber::set_global_default(subscriber).expect("Unable to set default tracing subscriber");
    }

    info!(">>>>>>>>>>>>>>>>>> New Session <<<<<<<<<<<<<<<<<<");

    if cli.parse {
        info!("starting server (single parse mode)");
        let backend = CliBackend::new(cli);
        backend.run();
    } else {
        if use_debug {
            info!(tag = "test", "starting server (debug mode)");
            let mut serv = Server::new_tcp().expect("Unable to start tcp connection");
            serv.initialize().expect("Error while initializing server");
            let sender_panic = serv.connection.as_ref().unwrap().sender.clone();
            std::panic::set_hook(Box::new(move |panic_info| {
                panic_hook(panic_info);
                let _ = sender_panic.send(lsp_server::Message::Notification(Notification{
                    method: "Odoo/displayCrashNotification".to_string(),
                    params: json!({
                        "crashInfo": format!("{panic_info}"),
                        "pid": std::process::id()
                    })
                }));
            }));
            serv.run(cli.clientProcessId);
        } else {
            info!("starting server");
            let mut serv = Server::new_stdio();
            serv.initialize().expect("Error while initializing server");
            let sender_panic = serv.connection.as_ref().unwrap().sender.clone();
            std::panic::set_hook(Box::new(move |panic_info| {
                panic_hook(panic_info);
                let _ = sender_panic.send(lsp_server::Message::Notification(Notification{
                    method: "Odoo/displayCrashNotification".to_string(),
                    params: json!({
                        "crashInfo": format!("{panic_info}"),
                        "pid": std::process::id()
                    })
                }));
            }));
            serv.run(cli.clientProcessId);
        }
    }
    info!(">>>>>>>>>>>>>>>>>> End Session <<<<<<<<<<<<<<<<<<");
}