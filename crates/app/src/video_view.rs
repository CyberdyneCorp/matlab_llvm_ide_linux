//! In-IDE playback for `VideoWriter` output. This host has no GTK4 GStreamer
//! media backend, so rather than `gtk::Video` we decode the file to PNG frames
//! with `ffmpeg` (already a runtime dependency for video export) and animate
//! them in a `Picture` with play/pause and a scrubber.

use std::cell::{Cell, RefCell};
use std::hash::{Hash, Hasher};
use std::path::Path;
use std::process::Command;
use std::rc::Rc;
use std::time::Duration;

use gtk::glib;
use gtk::prelude::*;
use gtk::{Box as GtkBox, Button, Label, Orientation, Picture, Scale, Window};

use crate::app_state::AppState;

const MAX_FRAMES: usize = 600;
const MAX_WIDTH: u32 = 720;

/// Decode `path` and open a player window. Shows a toast and returns if the
/// video can't be decoded (e.g. `ffmpeg` missing or an unsupported codec).
pub fn open(app: &Rc<AppState>, path: &Path) {
    let frames: Vec<gtk::gdk::Texture> = match decode_frames(path) {
        Some(f) if !f.is_empty() => f,
        _ => {
            app.vm.toast.show("Could not decode the video (needs ffmpeg)");
            return;
        }
    };
    let fps = probe_fps(path).unwrap_or(20.0).clamp(1.0, 60.0);
    let n = frames.len();
    let frames = Rc::new(frames);

    let name = path.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let window = Window::builder()
        .title(format!("Video — {name}"))
        .default_width(760)
        .default_height(620)
        .build();
    window.add_css_class("mf-root");
    if let Some(parent) = crate::ui::main_window() {
        window.set_transient_for(Some(&parent));
    }

    let root = GtkBox::new(Orientation::Vertical, 0);
    root.add_css_class("mf-window");

    let picture = Picture::new();
    picture.set_content_fit(gtk::ContentFit::Contain);
    picture.set_hexpand(true);
    picture.set_vexpand(true);
    picture.set_paintable(Some(&frames[0]));
    root.append(&picture);

    // Transport: play/pause, scrubber, frame counter.
    let bar = GtkBox::new(Orientation::Horizontal, 6);
    bar.add_css_class("mf-flow-toolbar");
    let play = Button::from_icon_name("media-playback-pause-symbolic");
    play.add_css_class("mf-header-action");
    play.set_tooltip_text(Some("Play / pause"));
    let scale = Scale::with_range(Orientation::Horizontal, 0.0, (n.max(1) - 1) as f64, 1.0);
    scale.set_draw_value(false);
    scale.set_hexpand(true);
    let counter = Label::new(Some(&format!("1/{n}")));
    counter.add_css_class("mf-text-muted");
    counter.set_width_chars(9);
    bar.append(&play);
    bar.append(&scale);
    bar.append(&counter);
    root.append(&bar);
    window.set_child(Some(&root));

    let timer: Rc<RefCell<Option<glib::SourceId>>> = Rc::new(RefCell::new(None));
    let playing = Rc::new(Cell::new(true));

    // The scale is the single source of truth for the current frame: changing it
    // updates the picture + counter, whether the move came from playback or drag.
    {
        let frames = frames.clone();
        let picture = picture.clone();
        let counter = counter.clone();
        scale.connect_value_changed(move |s| {
            let i = (s.value().round() as usize).min(frames.len().saturating_sub(1));
            picture.set_paintable(Some(&frames[i]));
            counter.set_text(&format!("{}/{}", i + 1, frames.len()));
        });
    }

    let start_timer = {
        let scale = scale.clone();
        let timer = timer.clone();
        move || {
            let interval = Duration::from_millis((1000.0 / fps) as u64);
            let scale = scale.clone();
            let id = glib::timeout_add_local(interval, move || {
                let next = (scale.value().round() as usize + 1) % n.max(1);
                scale.set_value(next as f64);
                glib::ControlFlow::Continue
            });
            *timer.borrow_mut() = Some(id);
        }
    };
    let stop_timer = {
        let timer = timer.clone();
        move || {
            if let Some(id) = timer.borrow_mut().take() {
                id.remove();
            }
        }
    };

    {
        let playing = playing.clone();
        let start = start_timer.clone();
        let stop = stop_timer.clone();
        play.connect_clicked(move |b| {
            if playing.get() {
                playing.set(false);
                stop();
                b.set_icon_name("media-playback-start-symbolic");
            } else {
                playing.set(true);
                start();
                b.set_icon_name("media-playback-pause-symbolic");
            }
        });
    }
    // Dragging the scrubber pauses playback so it doesn't fight the user.
    {
        let playing = playing.clone();
        let stop = stop_timer.clone();
        let play = play.clone();
        let gesture = gtk::GestureClick::new();
        gesture.connect_pressed(move |_, _, _, _| {
            if playing.get() {
                playing.set(false);
                stop();
                play.set_icon_name("media-playback-start-symbolic");
            }
        });
        scale.add_controller(gesture);
    }
    // Stop the timer when the window closes.
    {
        let stop = stop_timer.clone();
        window.connect_close_request(move |_| {
            stop();
            glib::Propagation::Proceed
        });
    }

    start_timer();
    window.present();
}

/// Probe the video's frame rate via `ffprobe` (`avg_frame_rate` as `num/den`).
fn probe_fps(path: &Path) -> Option<f64> {
    let out = Command::new("ffprobe")
        .args([
            "-v", "error", "-select_streams", "v:0",
            "-show_entries", "stream=avg_frame_rate",
            "-of", "default=nw=1:nk=1",
        ])
        .arg(path)
        .output()
        .ok()?;
    let s = String::from_utf8_lossy(&out.stdout);
    let (num, den) = s.trim().split_once('/')?;
    let (num, den): (f64, f64) = (num.trim().parse().ok()?, den.trim().parse().ok()?);
    (den != 0.0).then_some(num / den)
}

/// Decode `path` to PNG frames in a per-file temp dir and load them as textures.
fn decode_frames(path: &Path) -> Option<Vec<gtk::gdk::Texture>> {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    path.hash(&mut hasher);
    let dir = std::env::temp_dir().join(format!("matforge-video-{:x}", hasher.finish()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).ok()?;

    let status = Command::new("ffmpeg")
        .args(["-nostdin", "-v", "error", "-y", "-i"])
        .arg(path)
        .args(["-frames:v", &MAX_FRAMES.to_string()])
        .arg("-vf")
        .arg(format!("scale='min({MAX_WIDTH},iw)':-1"))
        .arg(dir.join("f_%05d.png"))
        .status()
        .ok()?;
    if !status.success() {
        return None;
    }

    let mut files: Vec<_> = std::fs::read_dir(&dir)
        .ok()?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|e| e == "png"))
        .collect();
    files.sort();
    Some(files.iter().filter_map(|p| gtk::gdk::Texture::from_filename(p).ok()).collect())
}
