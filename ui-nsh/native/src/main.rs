#![forbid(unsafe_code)]

use std::{path::Path, process::ExitCode};

use hd_project::{Grant, parse_manifest};
use hd_run::{Engine as _, GrantValue, Grants, HostSetup, Instance as _, Limits, Outcome};
use hd_run_wasmtime::{EngineConfig, HostIo, WasmtimeEngine};
#[cfg(target_os = "linux")]
use nsh_platform_linux::{AppOptions, AppView, Root, gtk4 as gtk};
#[cfg(target_os = "macos")]
use nsh_platform_macos::{AppOptions, AppSource, Root, quit_app, run_app};

use surface::{Batch, Provider};
use view::{Mount, NshSurface};

mod abi {
    include!(env!("HDRU_NSH_BINDINGS"));
}
mod style;
mod surface;
mod tree;
mod view;

const NAME: &str = env!("HDRU_NSH_NAME");
const MANIFEST: &str = env!("HDRU_NSH_MANIFEST");
const APP_ID: &str = "org.hdru.NativeShell";

fn guest(provider: Provider) -> Result<u8, String> {
    let engine = WasmtimeEngine::new(EngineConfig::default()).map_err(|e| format!("{e:?}"))?;
    let module = engine
        .load(
            include_bytes!(env!("HDRU_NSH_WASM")),
            &hd_cache::MemoryStore::default(),
        )
        .map_err(|e| format!("load {NAME}: {e:?}"))?;

    let manifest = parse_manifest(include_str!(env!("HDRU_NSH_MANIFEST")))
        .map_err(|e| format!("read {MANIFEST}: {e:?}"))?;
    let capabilities = manifest
        .capabilities
        .into_iter()
        .filter_map(|(key, grant, _)| {
            let value = match grant? {
                Grant::All => GrantValue::All,
                Grant::Deny => GrantValue::Deny,
                Grant::Scopes(list) => GrantValue::Scopes(list),
            };
            Some((key, value))
        })
        .collect::<Vec<_>>();
    let package = Path::new(MANIFEST).parent().expect("hd package directory");
    let host = HostSetup {
        grants: Grants::resolve(&capabilities, package, &[], package),
        limits: Limits::default(),
        args: Vec::new(),
        program: NAME.into(),
    };
    let bindings = abi::Bindings::builder()
        .surface(provider)
        .build()
        .map_err(|e| format!("{e:?}"))?;
    let mut instance = engine
        .instantiate_with_bindings(&module, &host, HostIo::process(), bindings)
        .map_err(|e| format!("start {NAME}: {e:?}"))?;
    match instance.init() {
        Outcome::Exit(0) => match hd_run::drive(&mut instance, usize::MAX) {
            Outcome::Exit(status) => Ok(status),
            other => Err(format!("{NAME}: {other:?}")),
        },
        other => Err(format!("initialize {NAME}: {other:?}")),
    }
}

#[cfg(target_os = "linux")]
fn run_window(mount: Mount, finished: async_channel::Receiver<()>) -> ExitCode {
    use gtk::prelude::*;

    let app = gtk::Application::builder()
        .application_id(APP_ID)
        .flags(gtk::gio::ApplicationFlags::NON_UNIQUE)
        .build();
    app.connect_activate(move |app| {
        if let Some(window) = app.active_window() {
            window.present();
            return;
        }
        let window = gtk::ApplicationWindow::builder()
            .application(app)
            .title(NAME)
            .default_width(400)
            .default_height(260)
            .build();
        let view = AppView::new(AppOptions::default());
        let mount = mount.clone();
        view.load(Root::new::<NshSurface>(move || mount.clone()).into());
        window.set_child(Some(&view.widget()));
        window.connect_destroy(move |_| view.stop());
        window.present();
        let app = app.downgrade();
        let finished = finished.clone();
        gtk::glib::MainContext::default().spawn_local(async move {
            let _ = finished.recv().await;
            if let Some(app) = app.upgrade() {
                app.quit();
            }
        });
    });
    if app.run_with_args::<&str>(&[]) == gtk::glib::ExitCode::SUCCESS {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

#[cfg(target_os = "macos")]
fn run_window(mount: Mount, finished: async_channel::Receiver<()>) -> ExitCode {
    // Wait for the guest on the NSH engine, which starts after the AppKit
    // window is registered. The guest can fail before its first render.
    let root = Root::new::<NshSurface>(move || {
        let finished = finished.clone();
        nsh_core::prelude::spawn(async move {
            let _ = finished.recv().await;
            quit_app();
        });
        mount.clone()
    });
    run_app(
        AppSource::Rust(root),
        AppOptions {
            app_id: APP_ID.into(),
            title: NAME.into(),
            size: (400, 260),
            ..Default::default()
        },
    )
}

fn run() -> Result<u8, String> {
    let (provider, batches, events) = Provider::connect();
    let (done, finished) = async_channel::bounded::<()>(1);
    let worker = std::thread::spawn(move || {
        let _done = done;
        guest(provider)
    });
    let status = run_window(Mount::new(batches.clone(), events.clone()), finished);
    events.close();
    batches.close();
    while let Ok(Batch(_, completion)) = batches.try_recv() {
        completion.complete(Ok(()));
    }
    let guest = worker
        .join()
        .map_err(|_| format!("{NAME} worker panicked"))??;
    Ok(if status == ExitCode::SUCCESS {
        guest
    } else {
        1
    })
}

fn main() -> ExitCode {
    match run() {
        Ok(status) => ExitCode::from(status),
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
#[test]
fn native_counter_renders_handles_presses_and_shuts_down() {
    let (provider, batches, events) = Provider::connect();
    let mount = Mount::new(batches.clone(), events.clone());
    let root = Root::new::<NshSurface>(move || mount.clone());
    let mut config = nsh_engine::EngineConfig::new(root);
    config.environment.os = if cfg!(target_os = "macos") {
        nsh_core::platform::Os::MacOs
    } else {
        nsh_core::platform::Os::Linux
    };
    let mut app = nsh_testing::Headless::start(config).unwrap();
    let worker = std::thread::spawn(move || guest(provider));

    app.wait_for("Count: 0").unwrap();
    app.press("role:button[+]").unwrap();
    app.wait_for("Count: 1").unwrap();
    app.press("role:button[-]").unwrap();
    app.wait_for("Count: 0").unwrap();
    app.press_key("tab").unwrap();
    app.press_key("return").unwrap();
    app.wait_for("Count: -1").unwrap();

    events.close();
    assert_eq!(worker.join().unwrap().unwrap(), 0);
}
