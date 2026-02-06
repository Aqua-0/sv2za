mod backend;
mod cancel;
mod config;
mod fb;
mod paths;
mod progress;
mod template;
mod ui;

use anyhow::Context as _;
use clap::Parser;
use config::{AppConfig, HeadlessArgs};
use eframe::egui;
use progress::{ProgressEvent, ProgressSink};

fn main() -> anyhow::Result<()> {
    let args = HeadlessArgs::parse();

    if args.headless {
        let mut cfg = AppConfig::load_or_default()?;
        cfg.apply_headless(&args);

        let (sink, rx) = ProgressSink::new();
        let cancel = cancel::CancelToken::new();

        std::thread::spawn(move || {
            while let Ok(ev) = rx.recv() {
                print_headless_event(&ev);
            }
        });

        backend::run(&cfg, sink, cancel).context("backend run failed")?;
        return Ok(());
    }

    let icon = load_app_icon();
    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_icon(icon),
        ..Default::default()
    };
    eframe::run_native(
        "SV2ZA Converter",
        native_options,
        Box::new(|cc| {
            let cfg = AppConfig::load_or_default().unwrap_or_default();
            Box::new(ui::SvZaApp::new(cc, cfg))
        }),
    )
    .map_err(|e| anyhow::anyhow!("{e}"))?;

    Ok(())
}

fn load_app_icon() -> egui::IconData {
    let png = include_bytes!("../icon.png");
    eframe::icon_data::from_png_bytes(png).unwrap_or_default()
}

fn print_headless_event(ev: &ProgressEvent) {
    match ev {
        ProgressEvent::PhaseStart { name } => eprintln!("[phase] {name}"),
        ProgressEvent::Info { msg } => eprintln!("{msg}"),
        ProgressEvent::Warn { msg } => eprintln!("[warn] {msg}"),
        ProgressEvent::Error { msg } => eprintln!("[error] {msg}"),
        ProgressEvent::Progress { done, total } => {
            if *total > 0 {
                let pct = (*done as f32) * 100.0 / (*total as f32);
                eprintln!("[progress] {done}/{total} ({pct:.1}%)");
            }
        }
        ProgressEvent::PhaseEnd { name } => eprintln!("[done] {name}"),
        ProgressEvent::Finished { ok } => eprintln!("[finished] ok={ok}"),
    }
}
