//! egui/eframe UI for the dxQ desktop monitor.
//!
//! Four views over the collector's live state — posture summary,
//! inventory table, BLOCKED list, and an `org_q` trend — switched
//! from a left nav. Data arrives from the background poller in
//! [`crate::client`] via an `mpsc` channel; the app keeps the most
//! recent [`Snapshot`] and renders it.

use std::sync::mpsc::Receiver;
use std::time::Duration;

use eframe::egui::{self, Color32, Pos2, Sense, Stroke, Vec2};

use crate::client::{InventoryItem, Snapshot};

/// Quantum-readiness tier for an asset / org `q` in [0,1]. Thresholds
/// mirror the web dashboard: low q = urgent (classical, migrate now),
/// high q = ready.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tier {
    Urgent,
    Plan,
    Ready,
}

impl Tier {
    pub fn of(q: f32) -> Tier {
        if q < 0.34 {
            Tier::Urgent
        } else if q < 0.67 {
            Tier::Plan
        } else {
            Tier::Ready
        }
    }

    pub fn color(self) -> Color32 {
        match self {
            Tier::Urgent => Color32::from_rgb(0xdc, 0x26, 0x26), // red
            Tier::Plan => Color32::from_rgb(0xd9, 0x77, 0x06),   // amber
            Tier::Ready => Color32::from_rgb(0x16, 0xa3, 0x4a),  // green
        }
    }
}

pub fn q_color(q: f32) -> Color32 {
    Tier::of(q).color()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum View {
    Posture,
    Inventory,
    Blocked,
    Trend,
}

pub struct DesktopApp {
    collector: String,
    rx: Receiver<Snapshot>,
    snap: Snapshot,
    view: View,
}

impl DesktopApp {
    pub fn new(collector: String, rx: Receiver<Snapshot>) -> Self {
        Self {
            collector,
            rx,
            snap: Snapshot::default(),
            view: View::Posture,
        }
    }

    fn drain(&mut self) {
        // Keep only the latest snapshot if several landed between frames.
        while let Ok(s) = self.rx.try_recv() {
            self.snap = s;
        }
    }
}

impl eframe::App for DesktopApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.drain();

        egui::TopBottomPanel::top("header").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading("dxQ");
                ui.label("— the dimension of quantum readiness");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    match (&self.snap.error, self.snap.generation) {
                        (Some(e), _) => {
                            ui.colored_label(Tier::Urgent.color(), format!("⚠ {e}"));
                        }
                        (None, 0) => {
                            ui.weak("connecting…");
                        }
                        (None, g) => {
                            ui.colored_label(
                                Tier::Ready.color(),
                                format!("● live (poll #{g})"),
                            );
                        }
                    }
                    ui.separator();
                    ui.monospace(&self.collector);
                });
            });
        });

        egui::SidePanel::left("nav").resizable(false).show(ctx, |ui| {
            ui.add_space(6.0);
            let blocked_n = self.snap.blocked.len();
            ui.selectable_value(&mut self.view, View::Posture, "Posture");
            ui.selectable_value(
                &mut self.view,
                View::Inventory,
                format!("Inventory ({})", self.snap.inventory.len()),
            );
            ui.selectable_value(
                &mut self.view,
                View::Blocked,
                if blocked_n > 0 {
                    format!("⚠ BLOCKED ({blocked_n})")
                } else {
                    "BLOCKED (0)".to_string()
                },
            );
            ui.selectable_value(&mut self.view, View::Trend, "Trend");
        });

        egui::CentralPanel::default().show(ctx, |ui| match self.view {
            View::Posture => posture_view(ui, &self.snap),
            View::Inventory => inventory_table(ui, &self.snap.inventory, "No assets reported yet."),
            View::Blocked => {
                inventory_table(ui, &self.snap.blocked, "No BLOCKED assets — nothing forced into migration.")
            }
            View::Trend => trend_view(ui, &self.snap),
        });

        // Stay awake so the poller's data (and the liveness label)
        // refresh even while the user is idle.
        ctx.request_repaint_after(Duration::from_millis(750));
    }
}

fn posture_view(ui: &mut egui::Ui, snap: &Snapshot) {
    let Some(p) = &snap.posture else {
        ui.weak("Waiting for the first posture read…");
        return;
    };
    ui.add_space(8.0);
    ui.label("Org-level quantum-risk score");
    ui.heading(
        egui::RichText::new(format!("q = {:.2}", p.org_q))
            .size(48.0)
            .color(q_color(p.org_q)),
    );
    ui.add_space(8.0);
    egui::Grid::new("posture-grid").spacing([24.0, 6.0]).show(ui, |ui| {
        ui.label("Assets");
        ui.monospace(p.assets.to_string());
        ui.end_row();
        ui.label("BLOCKED");
        ui.colored_label(
            if p.blocked_count > 0 { Tier::Urgent.color() } else { Tier::Ready.color() },
            p.blocked_count.to_string(),
        );
        ui.end_row();
        ui.label("Deadline");
        ui.monospace(p.deadline.get(0..10).unwrap_or(&p.deadline));
        ui.end_row();
        ui.label("Horizon");
        ui.monospace(format!("{} years", p.horizon_years));
        ui.end_row();
    });
}

fn inventory_table(ui: &mut egui::Ui, items: &[InventoryItem], empty_msg: &str) {
    if items.is_empty() {
        ui.weak(empty_msg);
        return;
    }
    egui::ScrollArea::vertical().show(ui, |ui| {
        egui::Grid::new("inv-grid")
            .striped(true)
            .spacing([18.0, 4.0])
            .show(ui, |ui| {
                for h in ["KIND", "IDENTITY", "HOST", "q", "PQ", ""] {
                    ui.strong(h);
                }
                ui.end_row();
                for it in items {
                    ui.monospace(asset_kind_label(&it.asset_kind));
                    ui.monospace(&it.identity);
                    ui.monospace(it.host.as_deref().unwrap_or("—"));
                    ui.colored_label(q_color(it.q), format!("{:.2}", it.q));
                    ui.monospace(it.primitives.join(", "));
                    if it.blocked {
                        ui.colored_label(Tier::Urgent.color(), "⚠ BLOCKED");
                    } else {
                        ui.label("");
                    }
                    ui.end_row();
                }
            });
    });
}

fn asset_kind_label(k: &dxq_core::AssetKind) -> String {
    // serde snake_case is the canonical wire label; reuse it.
    serde_json::to_value(k)
        .ok()
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_else(|| "unknown".into())
}

fn trend_view(ui: &mut egui::Ui, snap: &Snapshot) {
    ui.label("org_q over time (top = low risk)");
    ui.add_space(4.0);
    if snap.history.len() < 2 {
        ui.weak("Collecting trend — at least two posture snapshots are needed to draw a line.");
        return;
    }
    let series: Vec<f32> = snap.history.iter().map(|s| s.org_q).collect();
    let (resp, painter) =
        ui.allocate_painter(Vec2::new(ui.available_width(), 220.0), Sense::hover());
    let rect = resp.rect;
    let pad = 12.0;
    let x0 = rect.left() + pad;
    let x1 = rect.right() - pad;
    let y0 = rect.top() + pad;
    let y1 = rect.bottom() - pad;

    // Gridlines at q = 0.25 / 0.5 / 0.75 on the fixed [0,1] scale.
    let grid = Stroke::new(1.0, Color32::from_gray(90));
    for g in [0.25_f32, 0.5, 0.75] {
        let y = y1 - g * (y1 - y0);
        painter.line_segment([Pos2::new(x0, y), Pos2::new(x1, y)], grid);
    }

    let n = series.len();
    let last = *series.last().unwrap();
    let x_at = |i: usize| x0 + (i as f32) / ((n - 1) as f32) * (x1 - x0);

    // BLOCKED overlay, scaled to its own max so the shape is visible
    // even when counts are small; faint so org_q stays the headline.
    let max_blocked = snap
        .history
        .iter()
        .map(|s| s.blocked_count)
        .max()
        .unwrap_or(0)
        .max(1) as f32;
    let blocked_pts: Vec<Pos2> = snap
        .history
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let frac = s.blocked_count as f32 / max_blocked;
            Pos2::new(x_at(i), y1 - frac * (y1 - y0))
        })
        .collect();
    painter.add(egui::Shape::line(
        blocked_pts,
        Stroke::new(1.5, Tier::Urgent.color().gamma_multiply(0.5)),
    ));

    // org_q line (headline), colored by the latest tier.
    let pts: Vec<Pos2> = series
        .iter()
        .enumerate()
        .map(|(i, &q)| Pos2::new(x_at(i), y1 - q.clamp(0.0, 1.0) * (y1 - y0)))
        .collect();
    painter.add(egui::Shape::line(pts, Stroke::new(2.0, q_color(last))));

    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui.colored_label(q_color(last), "—");
        ui.weak("org_q");
        ui.separator();
        ui.colored_label(Tier::Urgent.color().gamma_multiply(0.5), "—");
        ui.weak(format!("BLOCKED (0–{})", max_blocked as usize));
    });
    ui.weak(format!("{} points · latest q = {:.2}", n, last));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tiers_partition_the_range() {
        assert_eq!(Tier::of(0.0), Tier::Urgent);
        assert_eq!(Tier::of(0.33), Tier::Urgent);
        assert_eq!(Tier::of(0.34), Tier::Plan);
        assert_eq!(Tier::of(0.5), Tier::Plan);
        assert_eq!(Tier::of(0.66), Tier::Plan);
        assert_eq!(Tier::of(0.67), Tier::Ready);
        assert_eq!(Tier::of(1.0), Tier::Ready);
    }

    #[test]
    fn colors_are_distinct_per_tier() {
        let (u, p, r) = (
            Tier::Urgent.color(),
            Tier::Plan.color(),
            Tier::Ready.color(),
        );
        assert_ne!(u, p);
        assert_ne!(p, r);
        assert_ne!(u, r);
    }

    #[test]
    fn asset_kind_label_is_snake_case() {
        assert_eq!(asset_kind_label(&dxq_core::AssetKind::SshSession), "ssh_session");
        assert_eq!(asset_kind_label(&dxq_core::AssetKind::X509Cert), "x509_cert");
    }
}
