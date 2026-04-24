#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpStream, ToSocketAddrs};
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::{Duration, Instant};

use chrono::Local;
use eframe::egui::{self, Color32, RichText};
use url::Url;
use windows_sys::Win32::NetworkManagement::IpHelper::{
    ICMP_ECHO_REPLY, IP_FLAG_DF, IP_OPTION_INFORMATION, IP_SUCCESS, IcmpCloseHandle,
    IcmpCreateFile, IcmpSendEcho,
};

const DYNAMIC_THRESHOLD_MS: i64 = 100;
const PINGS_PER_ENDPOINT: usize = 20;
const SITE_ATTEMPTS: usize = 20;
const REFRESH_INTERVAL: Duration = Duration::from_secs(5);
const OVERLAY_WIDTH: f32 = 360.0;
const OVERLAY_HEIGHT: f32 = 312.0;
const MINIMIZED_HEIGHT: f32 = 54.0;
const OVERLAY_MARGIN: f32 = 8.0;
const UI_IDLE_REPAINT: Duration = Duration::from_millis(1000);
const UI_HOVER_REPAINT: Duration = Duration::from_millis(250);
const PING_TIMEOUT_MS: u32 = 500;
const PING_TTL: u8 = 64;
const PING_PAYLOAD_SIZE: usize = 32;

const ENDPOINTS: &[Endpoint] = &[Endpoint {
    name: "Google DNS",
    address: "8.8.8.8",
}];

const SITES: &[SiteTarget] = &[
    SiteTarget {
        name: "Chrono",
        url: "https://atkins.chronowms.com",
    },
    SiteTarget {
        name: "Payruler",
        url: "https://access2.payruler.com/",
    },
    SiteTarget {
        name: "Acumatica",
        url: "http://151.101.65.91",
    },
    SiteTarget {
        name: "BAI NVQSD",
        url: "https://nvqsd.bai.gov.ph/Livestock/EsRegA.aspx",
    },
];

#[derive(Clone, Copy)]
struct Endpoint {
    name: &'static str,
    address: &'static str,
}

#[derive(Clone, Copy)]
struct SiteTarget {
    name: &'static str,
    url: &'static str,
}

#[derive(Clone)]
struct MonitorSnapshot {
    captured_at: String,
    network: NetworkResult,
    sites: Vec<SiteResult>,
}

#[derive(Clone)]
struct NetworkResult {
    label: String,
    avg_ms: i64,
    loss_percent: u32,
    speed: String,
    trend: String,
}

#[derive(Clone)]
struct SiteResult {
    name: String,
    avg_ms: i64,
    loss_percent: u32,
    status: String,
    trend: String,
}

#[derive(Clone, Copy)]
struct PingConfig {
    timeout_ms: u32,
    ttl: u8,
    dont_fragment: bool,
    payload_size: usize,
}

#[derive(Default)]
struct MonitorApp {
    rx: Option<Receiver<MonitorSnapshot>>,
    latest: Option<MonitorSnapshot>,
    error_message: Option<String>,
    last_refresh: Option<Instant>,
    compact_mode: bool,
    click_through: bool,
    anchored: bool,
    minimized: bool,
}

impl MonitorApp {
    fn new(rx: Receiver<MonitorSnapshot>) -> Self {
        Self {
            rx: Some(rx),
            latest: None,
            error_message: None,
            last_refresh: None,
            compact_mode: true,
            click_through: false,
            anchored: false,
            minimized: false,
        }
    }

    fn poll_updates(&mut self) {
        let Some(rx) = &self.rx else {
            return;
        };

        while let Ok(snapshot) = rx.try_recv() {
            self.latest = Some(snapshot);
            self.last_refresh = Some(Instant::now());
            self.error_message = None;
        }
    }
}

const DEFAULT_PING_CONFIG: PingConfig = PingConfig {
    timeout_ms: PING_TIMEOUT_MS,
    ttl: PING_TTL,
    dont_fragment: false,
    payload_size: PING_PAYLOAD_SIZE,
};

impl eframe::App for MonitorApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll_updates();
        anchor_overlay(ctx, &mut self.anchored);
        let hovered = ctx.is_pointer_over_area();
        ctx.request_repaint_after(if hovered {
            UI_HOVER_REPAINT
        } else {
            UI_IDLE_REPAINT
        });
        let panel_fill = if hovered {
            Color32::from_rgba_unmultiplied(10, 14, 22, 196)
        } else {
            Color32::from_rgba_unmultiplied(10, 14, 22, 72)
        };
        let border = if hovered {
            Color32::from_rgba_unmultiplied(90, 130, 150, 120)
        } else {
            Color32::from_rgba_unmultiplied(90, 130, 150, 55)
        };

        egui::CentralPanel::default()
            .show(ctx, |ui| {
                ui.visuals_mut().override_text_color = Some(Color32::WHITE);
                handle_shortcuts(ctx, self);
                egui::Frame::default()
                    .fill(panel_fill)
                    .corner_radius(10.0)
                    .stroke(egui::Stroke::new(1.0, border))
                    .inner_margin(egui::Margin::same(12))
                    .show(ui, |ui| {
                        render_overlay_header(ui, ctx, self);

                        if self.minimized {
                            if let Some(snapshot) = &self.latest {
                                let net = &snapshot.network;
                                ui.label(
                                    RichText::new(format!(
                                        "{} [{} ms]  loss {}%",
                                        net.label,
                                        display_ms(net.avg_ms),
                                        net.loss_percent
                                    ))
                                    .monospace()
                                    .size(12.0)
                                    .color(status_color(&net.label)),
                                );
                            } else {
                                ui.label(RichText::new("Collecting...").monospace().size(12.0));
                            }
                            return;
                        }

                        egui::ScrollArea::vertical()
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                ui.label(
                                    RichText::new("F8 toggle click-through  •  F9 toggle compact mode")
                                        .size(11.0),
                                );
                                ui.add_space(8.0);

                                if let Some(snapshot) = &self.latest {
                                    ui.label(format!("Updated: {}", snapshot.captured_at));
                                    if let Some(last_refresh) = self.last_refresh {
                                        ui.label(format!(
                                            "Last refresh: {}s ago",
                                            last_refresh.elapsed().as_secs()
                                        ));
                                    }
                                    ui.add_space(12.0);

                                    if self.compact_mode {
                                        render_compact_network(ui, &snapshot.network);
                                        ui.add_space(6.0);
                                        for site in &snapshot.sites {
                                            render_compact_site(ui, site);
                                            ui.add_space(4.0);
                                        }
                                    } else {
                                        render_metric_block(
                                            ui,
                                            "Network Status",
                                            &snapshot.network.label,
                                            snapshot.network.avg_ms,
                                            &snapshot.network.speed,
                                            snapshot.network.loss_percent,
                                            &snapshot.network.trend,
                                        );

                                        ui.add_space(8.0);

                                        for site in &snapshot.sites {
                                            render_metric_block(
                                                ui,
                                                &format!("{} Status", site.name),
                                                &site.status,
                                                site.avg_ms,
                                                &site.status,
                                                site.loss_percent,
                                                &site.trend,
                                            );
                                            ui.add_space(8.0);
                                        }
                                    }
                                } else {
                                    ui.label("Collecting initial network measurements...");
                                    ui.label(
                                        "The first snapshot appears as soon as the first monitoring pass finishes.",
                                    );
                                }

                                if let Some(error) = &self.error_message {
                                    ui.add_space(8.0);
                                    ui.colored_label(Color32::LIGHT_RED, error);
                                }
                            });
                    });
            });
    }
}

fn render_overlay_header(ui: &mut egui::Ui, ctx: &egui::Context, app: &mut MonitorApp) {
    ui.horizontal(|ui| {
        ui.heading(RichText::new("Network Overlay").size(19.0).strong());
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let kill = egui::Button::new(RichText::new("X").strong().color(Color32::WHITE))
                .fill(Color32::from_rgb(160, 40, 40));
            if ui.add(kill).on_hover_text("Close overlay").clicked() {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }

            let minimize_label = if app.minimized { "+" } else { "-" };
            if ui
                .button(minimize_label)
                .on_hover_text("Minimize or restore overlay")
                .clicked()
            {
                app.minimized = !app.minimized;
                resize_overlay(ctx, app.minimized, app.compact_mode);
                app.anchored = false;
            }

            let mode = if app.click_through {
                "click-through on"
            } else {
                "click-through off"
            };
            ui.label(RichText::new(mode).size(11.0).color(Color32::LIGHT_GRAY));
        });
    });
}

fn handle_shortcuts(ctx: &egui::Context, app: &mut MonitorApp) {
    if ctx.input(|input| input.key_pressed(egui::Key::F8)) {
        app.click_through = !app.click_through;
        ctx.send_viewport_cmd(egui::ViewportCommand::MousePassthrough(app.click_through));
    }

    if ctx.input(|input| input.key_pressed(egui::Key::F9)) {
        app.compact_mode = !app.compact_mode;
        resize_overlay(ctx, app.minimized, app.compact_mode);
        app.anchored = false;
    }
}

fn resize_overlay(ctx: &egui::Context, minimized: bool, compact_mode: bool) {
    let new_height = if minimized {
        MINIMIZED_HEIGHT
    } else if compact_mode {
        OVERLAY_HEIGHT
    } else {
        560.0
    };

    ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(
        OVERLAY_WIDTH,
        new_height,
    )));
}

fn anchor_overlay(ctx: &egui::Context, anchored: &mut bool) {
    if *anchored {
        return;
    }

    let screen_rect = ctx.input(|input| input.content_rect());
    if screen_rect.width() <= 0.0 || screen_rect.height() <= 0.0 {
        return;
    }

    let pos = egui::pos2(
        screen_rect.right() - OVERLAY_WIDTH - OVERLAY_MARGIN,
        screen_rect.top() + OVERLAY_MARGIN,
    );
    ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(pos));
    *anchored = true;
}

fn render_compact_network(ui: &mut egui::Ui, network: &NetworkResult) {
    let title = format!(
        "NET {} [{} ms]  loss {}%  {}  {}",
        network.label,
        display_ms(network.avg_ms),
        network.loss_percent,
        network.speed,
        network.trend
    );
    render_compact_row(ui, &title, status_color(&network.label));
}

fn render_compact_site(ui: &mut egui::Ui, site: &SiteResult) {
    let summary = format!(
        "{}: {} [{} ms]  loss {}%  {}",
        site.name,
        site.status,
        display_ms(site.avg_ms),
        site.loss_percent,
        site.trend
    );
    render_compact_row(ui, &summary, status_color(&site.status));
}

fn render_compact_row(ui: &mut egui::Ui, text: &str, accent: Color32) {
    egui::Frame::group(ui.style())
        .fill(Color32::from_rgba_unmultiplied(15, 21, 31, 168))
        .stroke(egui::Stroke::new(1.0, Color32::from_rgba_unmultiplied(70, 100, 120, 120)))
        .corner_radius(8.0)
        .show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.colored_label(accent, "■");
                ui.label(RichText::new(text).monospace().size(12.0));
            });
        });
}

fn render_metric_block(
    ui: &mut egui::Ui,
    title: &str,
    label: &str,
    avg_ms: i64,
    speed: &str,
    loss_percent: u32,
    trend: &str,
) {
    egui::Frame::group(ui.style())
        .fill(Color32::from_rgba_unmultiplied(15, 21, 31, 180))
        .corner_radius(8.0)
        .show(ui, |ui| {
            ui.label(RichText::new(title).strong());
            ui.horizontal(|ui| {
                ui.label("Status:");
                ui.colored_label(status_color(label), format!("{label} [{} ms]", display_ms(avg_ms)));
            });
            ui.horizontal(|ui| {
                ui.label("Speed:");
                ui.colored_label(status_color(speed), speed);
            });
            ui.horizontal(|ui| {
                ui.label("Loss:");
                ui.colored_label(loss_color(loss_percent), format!("{loss_percent}%"));
            });
            ui.horizontal(|ui| {
                ui.label("Trend:");
                ui.colored_label(status_color(trend), trend);
            });
        });
}

fn display_ms(avg_ms: i64) -> String {
    if avg_ms >= 0 {
        avg_ms.to_string()
    } else {
        "down".to_owned()
    }
}

fn status_color(status: &str) -> Color32 {
    match status {
        "EXCELLENT" | "FAST" | "IMPROVING" => Color32::from_rgb(102, 255, 102),
        "GOOD" => Color32::from_rgb(200, 255, 120),
        "FAIR" | "STABLE" => Color32::from_rgb(255, 225, 90),
        "POOR" | "SLOW" => Color32::from_rgb(255, 120, 90),
        "DOWN" | "DETERIORATING" => Color32::from_rgb(255, 80, 80),
        _ => Color32::WHITE,
    }
}

fn loss_color(loss: u32) -> Color32 {
    match loss {
        0 => Color32::from_rgb(102, 255, 102),
        1..=10 => Color32::from_rgb(200, 255, 120),
        11..=30 => Color32::from_rgb(255, 225, 90),
        _ => Color32::from_rgb(255, 80, 80),
    }
}

fn get_level(ms: i64) -> &'static str {
    if ms < 0 {
        "DOWN"
    } else if ms <= 80 {
        "EXCELLENT"
    } else if ms <= 150 {
        "GOOD"
    } else if ms <= 300 {
        "FAIR"
    } else {
        "POOR"
    }
}

fn get_trend(latencies: &[i64]) -> &'static str {
    let good_samples: Vec<i64> = latencies.iter().copied().filter(|value| *value >= 0).collect();
    if good_samples.is_empty() {
        return "DOWN";
    }
    if good_samples.len() < 2 {
        return "STABLE";
    }

    let previous = good_samples[good_samples.len() - 2];
    let current = good_samples[good_samples.len() - 1];

    if current < previous {
        "IMPROVING"
    } else if current > previous {
        "DETERIORATING"
    } else {
        "STABLE"
    }
}

fn median_trimmed(latencies: &mut Vec<i64>) -> i64 {
    if latencies.len() > 5 {
        latencies.sort_unstable();
        latencies.drain(latencies.len() - 2..);
        latencies.drain(..2);
    }

    if latencies.is_empty() {
        return -1;
    }

    latencies.sort_unstable();
    let mid = latencies.len() / 2;
    if latencies.len() % 2 == 0 {
        (latencies[mid - 1] + latencies[mid]) / 2
    } else {
        latencies[mid]
    }
}

fn extract_site_endpoint(url: &str) -> Option<(String, u16)> {
    let parsed = Url::parse(url).ok()?;
    let host = parsed.host_str()?.to_owned();
    let port = parsed.port_or_known_default()?;
    Some((host, port))
}

fn ping_once(target: &str) -> Option<i64> {
    let address = resolve_ipv4_target(target)?;
    ping_ipv4_native(address, DEFAULT_PING_CONFIG)
}

fn resolve_ipv4_target(target: &str) -> Option<Ipv4Addr> {
    if let Ok(ip) = target.parse::<Ipv4Addr>() {
        return Some(ip);
    }

    let addrs = (target, 0).to_socket_addrs().ok()?;
    addrs.into_iter().find_map(|addr| match addr.ip() {
        IpAddr::V4(ipv4) => Some(ipv4),
        IpAddr::V6(_) => None,
    })
}

fn ping_ipv4_native(ip: Ipv4Addr, config: PingConfig) -> Option<i64> {
    let handle = unsafe { IcmpCreateFile() };
    if handle.is_null() {
        return None;
    }

    let payload = vec![0u8; config.payload_size];
    let mut request_options = IP_OPTION_INFORMATION {
        Ttl: config.ttl,
        Tos: 0,
        Flags: if config.dont_fragment {
            IP_FLAG_DF as u8
        } else {
            0
        },
        OptionsSize: 0,
        OptionsData: std::ptr::null_mut(),
    };
    let reply_size = std::mem::size_of::<ICMP_ECHO_REPLY>() + payload.len() + 8;
    let mut reply_buffer = vec![0u8; reply_size];

    let result = unsafe {
        IcmpSendEcho(
            handle,
            u32::from_be_bytes(ip.octets()),
            payload.as_ptr().cast(),
            payload.len() as u16,
            &mut request_options,
            reply_buffer.as_mut_ptr().cast(),
            reply_buffer.len() as u32,
            config.timeout_ms,
        )
    };

    let latency = if result == 0 {
        None
    } else {
        let reply = unsafe { &*(reply_buffer.as_ptr().cast::<ICMP_ECHO_REPLY>()) };
        if reply.Status == IP_SUCCESS {
            Some(i64::from(reply.RoundTripTime))
        } else {
            None
        }
    };

    unsafe {
        IcmpCloseHandle(handle);
    }

    latency
}

fn test_isp(endpoints: &[Endpoint]) -> NetworkResult {
    let mut latencies = Vec::with_capacity(endpoints.len() * PINGS_PER_ENDPOINT);
    let mut total_sent = 0usize;
    let mut loss_count = 0usize;

    for endpoint in endpoints {
        let _ = endpoint.name;
        for _ in 0..PINGS_PER_ENDPOINT {
            match ping_once(endpoint.address) {
                Some(latency) => latencies.push(latency),
                None => loss_count += 1,
            }
            total_sent += 1;
        }
    }

    let avg_ms = median_trimmed(&mut latencies);
    let loss_percent = ((loss_count as f64 / total_sent.max(1) as f64) * 100.0).round() as u32;
    let speed = if avg_ms >= 0 && avg_ms < DYNAMIC_THRESHOLD_MS {
        "FAST"
    } else if avg_ms >= 0 {
        "SLOW"
    } else {
        "DOWN"
    };

    NetworkResult {
        label: get_level(avg_ms).to_owned(),
        avg_ms,
        loss_percent,
        speed: speed.to_owned(),
        trend: get_trend(&latencies).to_owned(),
    }
}

fn test_site(target: SiteTarget) -> SiteResult {
    let endpoint = extract_site_endpoint(target.url);
    let mut success_count = 0usize;
    let mut latencies = Vec::with_capacity(SITE_ATTEMPTS);

    if let Some((host, port)) = endpoint {
        for _ in 0..SITE_ATTEMPTS {
            if let Some(latency) = tcp_probe(&host, port, Duration::from_millis(700)) {
                success_count += 1;
                latencies.push(latency);
            }
        }
    }

    let avg_ms = median_trimmed(&mut latencies);
    let loss_percent =
        (((SITE_ATTEMPTS.saturating_sub(success_count)) as f64 / SITE_ATTEMPTS as f64) * 100.0)
            .round() as u32;
    let status = if avg_ms >= 0 && avg_ms <= DYNAMIC_THRESHOLD_MS {
        "FAST"
    } else if avg_ms >= 0 {
        "SLOW"
    } else {
        "DOWN"
    };

    SiteResult {
        name: target.name.to_owned(),
        avg_ms,
        loss_percent,
        status: status.to_owned(),
        trend: get_trend(&latencies).to_owned(),
    }
}

fn quick_snapshot() -> MonitorSnapshot {
    let captured_at = timestamp_now();

    let network_handle = thread::spawn(|| test_isp(ENDPOINTS));
    let site_handles: Vec<_> = SITES
        .iter()
        .copied()
        .map(|site| thread::spawn(move || test_site(site)))
        .collect();

    let network = network_handle.join().unwrap_or_else(|_| NetworkResult {
        label: "DOWN".to_owned(),
        avg_ms: -1,
        loss_percent: 100,
        speed: "DOWN".to_owned(),
        trend: "DOWN".to_owned(),
    });

    let sites = site_handles
        .into_iter()
        .filter_map(|handle| handle.join().ok())
        .collect();

    MonitorSnapshot {
        captured_at,
        network,
        sites,
    }
}

fn timestamp_now() -> String {
    Local::now().format("%Y-%m-%d %I:%M %p").to_string()
}

fn tcp_probe(host: &str, port: u16, timeout: Duration) -> Option<i64> {
    let addrs = (host, port).to_socket_addrs().ok()?;
    tcp_probe_addrs(addrs, timeout)
}

fn tcp_probe_addrs(addrs: impl IntoIterator<Item = SocketAddr>, timeout: Duration) -> Option<i64> {
    for addr in addrs {
        let start = Instant::now();
        if TcpStream::connect_timeout(&addr, timeout).is_ok() {
            return Some(start.elapsed().as_millis() as i64);
        }
    }

    None
}

fn spawn_monitor_loop() -> Receiver<MonitorSnapshot> {
    let (tx, rx) = mpsc::channel();

    thread::spawn(move || {
        let initial = quick_snapshot();
        if tx.send(initial).is_err() {
            return;
        }

        loop {
            thread::sleep(REFRESH_INTERVAL);
            let snapshot = quick_snapshot();
            if tx.send(snapshot).is_err() {
                return;
            }
        }
    });

    rx
}

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Network Monitor")
            .with_inner_size([OVERLAY_WIDTH, OVERLAY_HEIGHT])
            .with_resizable(false)
            .with_always_on_top()
            .with_decorations(false)
            .with_transparent(true)
            .with_taskbar(false),
        ..Default::default()
    };

    let rx = spawn_monitor_loop();

    eframe::run_native(
        "Network Monitor",
        options,
        Box::new(move |_cc| Ok(Box::new(MonitorApp::new(rx)))),
    )
}
