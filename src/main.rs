use argh::FromArgs;
use env_logger::Env;
use log::{error, info};
use notify::{Config as NotifyConfig, Event, PollWatcher, RecursiveMode, Watcher};
use notify_rust::{error::Error as NotifyError, Notification, Timeout, Urgency};
use std::{
    error::Error,
    path::{Path, PathBuf},
    sync::mpsc,
    thread,
    time::Duration,
};

#[derive(FromArgs)]
#[argh(description = "A simple backlight notification daemon")]
struct Config {
    #[argh(
        option,
        default = "String::from(\"Blight\")",
        short = 't',
        description = "set notification title"
    )]
    title: String,
    #[argh(
        option,
        default = "String::from(\"Brightness adjusted:\")",
        short = 'm',
        description = "set notification message"
    )]
    message: String,
    #[argh(option, short = 'i', description = "set icon name/location")]
    icon: Option<String>,
    #[argh(
        option,
        short = 'T',
        default = "1000",
        description = "set notification timeout in milliseconds"
    )]
    timeout: u32,
    #[argh(
        option,
        short = 'p',
        default = "0.5f32",
        description = "set backlight change watcher polling rate"
    )]
    pollrate: f32,
    #[argh(switch, short = 'q', description = "disable logging")]
    quiet: bool,
    #[argh(switch, short = 'd', description = "enable debug level logging")]
    debug: bool,
}

fn main() -> Result<(), Box<dyn Error>> {
    let conf: Config = argh::from_env();
    if !conf.quiet {
        init_logging(conf.debug);
    }
    let (mut watcher, r) = init_watcher(conf.pollrate)?;
    watch(&mut watcher)?;
    loop {
        let v = r.recv()?;
        let spam = if let Ok(x) = r.try_recv() {
            let s = (0..10)
                .filter_map(|_| {
                    thread::sleep(Duration::from_millis(150));
                    r.try_recv().ok()
                })
                .last();
            s.or(Some(x))
        } else {
            None
        };

        let fval = spam.unwrap_or(v);
        let message = format!("{} {}%", conf.message, (fval * 100.) as u8);
        if let Err(error) = notify(&message, &conf.title, conf.icon.as_ref(), conf.timeout) {
            error!("{error}");
        }
    }
}

fn init_logging(debug: bool) {
    let level = if debug { "debug" } else { "info" };
    let env = Env::new().filter_or("RUST_LOG", level);
    env_logger::init_from_env(env);
    info!("blight-notify daemon started");
}

fn notify(
    message: &str,
    title: &str,
    icon: Option<&String>,
    timeout: u32,
) -> Result<(), NotifyError> {
    let mut notif = Notification::new();
    notif
        .timeout(Timeout::Milliseconds(timeout))
        .urgency(Urgency::Low)
        .id(696969)
        .appname("Blight notify")
        .summary(title)
        .body(message);
    if let Some(icon_path) = icon {
        notif.icon(&icon_path);
    } else {
        notif.auto_icon();
    }
    notif.show()?;
    Ok(())
}

fn watch(watcher: &mut impl Watcher) -> notify::Result<()> {
    let bl_paths: Vec<PathBuf> = std::fs::read_dir("/sys/class/backlight")
        .unwrap()
        .filter_map(|r| r.ok())
        .map(|e| {
            let mut p = e.path();
            p.push("brightness");
            p
        })
        .collect();

    for p in bl_paths {
        watcher.watch(&p, RecursiveMode::NonRecursive)?;
        info!("watching: {}", p.display());
    }
    Ok(())
}

fn init_watcher(poll_rate: f32) -> notify::Result<(impl Watcher, mpsc::Receiver<f64>)> {
    let (s, r) = mpsc::channel::<f64>();
    let watcher = PollWatcher::new(
        move |ev| handler(ev, s.clone()),
        NotifyConfig::default()
            .with_compare_contents(true)
            .with_poll_interval(Duration::from_secs_f32(poll_rate)),
    )
    .unwrap();
    Ok((watcher, r))
}

fn handler(ev: notify::Result<Event>, s: mpsc::Sender<f64>) {
    let read_val = |path: &Path| {
        std::fs::read_to_string(path)
            .unwrap()
            .trim()
            .parse()
            .unwrap()
    };

    if let Ok(mut event) = ev {
        let mut p = event.paths.pop().unwrap();
        let b: f64 = read_val(&p);
        p.set_file_name("max_brightness");
        let max: f64 = read_val(&p);
        let perc = b / max;
        s.send(perc).unwrap();
    }
}

// TODO: Use logging
