//! Software Update window.

use clouddeck_core::release::schedule::{UpdateState, headline, progress_fraction};
use clouddeck_core::release::target::current_version;
use gpui::{
    AnyElement, App, Bounds, ClickEvent, Context, Entity, Global, IntoElement, ParentElement,
    Render, SharedString, Styled, Subscription, TitlebarOptions, Window, WindowBounds,
    WindowHandle, WindowOptions, div, point, prelude::*, px, relative, size, white,
};
use gpui_component::Root;

use crate::tokens::{Palette, palette};
use crate::updater::{Updater, UpdaterEvent};

const WINDOW_SIZE: (f32, f32) = (440.0, 220.0);
const NOTES_MAX_HEIGHT: f32 = 108.0;

struct RegisteredUpdater(Entity<Updater>);

impl Global for RegisteredUpdater {}

struct OpenUpdateWindow(WindowHandle<Root>);

impl Global for OpenUpdateWindow {}

/// Makes the updater reachable from the menu and opens the window when a check finds an update.
pub fn register(cx: &mut App, updater: &Entity<Updater>) {
    cx.set_global(RegisteredUpdater(updater.clone()));
    cx.subscribe(updater, |updater, _: &UpdaterEvent, cx| {
        if matches!(updater.read(cx).state(), UpdateState::Available(_)) {
            open_window(cx, &updater);
        }
    })
    .detach();
}

/// Opens the update window and starts a manual check.
pub fn check_for_updates(cx: &mut App) {
    let Some(updater) = cx
        .try_global::<RegisteredUpdater>()
        .map(|registered| registered.0.clone())
    else {
        return;
    };
    open_window(cx, &updater);
    updater.update(cx, |updater, cx| updater.check(cx));
}

fn open_window(cx: &mut App, updater: &Entity<Updater>) -> Option<WindowHandle<Root>> {
    if let Some(handle) = cx.try_global::<OpenUpdateWindow>().map(|open| open.0)
        && handle
            .update(cx, |_, window, _| window.activate_window())
            .is_ok()
    {
        return Some(handle);
    }
    let bounds = Bounds::centered(None, size(px(WINDOW_SIZE.0), px(WINDOW_SIZE.1)), cx);
    let updater = updater.clone();
    let handle = cx
        .open_window(
            WindowOptions {
                titlebar: Some(TitlebarOptions {
                    title: Some(SharedString::from("Software Update")),
                    appears_transparent: true,
                    traffic_light_position: Some(point(px(14.0), px(15.0))),
                }),
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                app_id: Some(crate::main_window::APP_ID.to_string()),
                ..Default::default()
            },
            move |window, cx| {
                let view = cx.new(|cx| UpdateWindowView::new(updater, cx));
                cx.new(|cx| Root::new(view, window, cx))
            },
        )
        .ok()?;
    cx.set_global(OpenUpdateWindow(handle));
    Some(handle)
}

struct UpdateWindowView {
    updater: Entity<Updater>,
    _observation: Subscription,
}

impl UpdateWindowView {
    fn new(updater: Entity<Updater>, cx: &mut Context<Self>) -> Self {
        let observation = cx.observe(&updater, |_, _, cx| cx.notify());
        Self {
            updater,
            _observation: observation,
        }
    }
}

impl Render for UpdateWindowView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = palette(cx);
        let state = self.updater.read(cx).state().clone();
        let current = current_version().to_string();
        let title_text = headline(&state, &current);
        let title = if matches!(state, UpdateState::Failed { .. }) {
            div()
                .debug_selector(|| "update.error".into())
                .text_color(palette.err)
                .child(title_text)
                .into_any_element()
        } else {
            title_text.into_any_element()
        };

        let notes = match &state {
            UpdateState::Available(update) => Some(
                div()
                    .id("update.notes")
                    .debug_selector(|| "update.notes".into())
                    .max_h(px(NOTES_MAX_HEIGHT))
                    .overflow_y_scroll()
                    .text_size(px(12.))
                    .text_color(palette.dim)
                    .child(update.notes.clone())
                    .into_any_element(),
            ),
            _ => None,
        };
        let progress = match &state {
            UpdateState::Downloading { .. } => Some(progress_bar(
                progress_fraction(&state).unwrap_or(0.0),
                &palette,
            )),
            _ => None,
        };
        let buttons: Vec<AnyElement> = match &state {
            UpdateState::Available(_) => vec![
                button("update.later", "Later", &palette)
                    .on_click(cx.listener(|_, _: &ClickEvent, window, _| window.remove_window()))
                    .into_any_element(),
                primary_button("update.install", "Install and Relaunch", &palette)
                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                        this.updater.update(cx, |updater, cx| updater.install(cx));
                    }))
                    .into_any_element(),
            ],
            UpdateState::ReadyToRelaunch { .. } => vec![
                primary_button("update.relaunch", "Relaunch", &palette)
                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                        this.updater.update(cx, |updater, cx| updater.relaunch(cx));
                    }))
                    .into_any_element(),
            ],
            UpdateState::UpToDate { .. } | UpdateState::Failed { .. } => vec![
                button("update.ok", "OK", &palette)
                    .on_click(cx.listener(|_, _: &ClickEvent, window, _| window.remove_window()))
                    .into_any_element(),
            ],
            _ => Vec::new(),
        };

        div()
            .size_full()
            .flex()
            .flex_col()
            .gap(px(12.))
            .p(px(20.))
            .bg(palette.win)
            .text_color(palette.text)
            .text_size(px(13.))
            .child(
                div()
                    .debug_selector(|| "update.title".into())
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .child(title),
            )
            .children(notes)
            .children(progress)
            .child(div().flex_1())
            .child(div().flex().justify_end().gap(px(8.)).children(buttons))
    }
}

fn progress_bar(fraction: f32, palette: &Palette) -> AnyElement {
    div()
        .debug_selector(|| "update.progress".into())
        .w_full()
        .h(px(6.))
        .rounded(px(3.))
        .bg(palette.line)
        .child(
            div()
                .h_full()
                .w(relative(fraction))
                .rounded(px(3.))
                .bg(palette.accent),
        )
        .into_any_element()
}

fn button(id: &'static str, label: &'static str, palette: &Palette) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id)
        .debug_selector(move || id.into())
        .px(px(12.))
        .py(px(4.))
        .rounded(px(6.))
        .border_1()
        .border_color(palette.line)
        .bg(palette.surf)
        .text_size(px(12.))
        .text_color(palette.text)
        .child(label)
}

fn primary_button(
    id: &'static str,
    label: &'static str,
    palette: &Palette,
) -> gpui::Stateful<gpui::Div> {
    button(id, label, palette)
        .bg(palette.accent)
        .border_color(palette.accent)
        .text_color(white())
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::Arc;

    use clouddeck_core::release::manifest::{Asset, Manifest, to_canonical_json};
    use clouddeck_core::release::sign::{generate_keypair, sign_bytes};
    use clouddeck_core::release::source::{FetchedRelease, UpdateSource};
    use clouddeck_core::release::target::Target;
    use clouddeck_core::release::verify::sha256_hex;
    use gpui::{TestAppContext, VisualTestContext};
    use semver::Version;

    use super::*;

    const ASSET: &[u8] = b"asset";

    struct FakeSource {
        release: Option<(Vec<u8>, String)>,
        asset: Vec<u8>,
    }

    impl UpdateSource for FakeSource {
        fn latest(&self) -> Result<FetchedRelease, String> {
            match &self.release {
                Some((manifest_bytes, signature_b64)) => Ok(FetchedRelease {
                    manifest_bytes: manifest_bytes.clone(),
                    signature_b64: signature_b64.clone(),
                }),
                None => Err("offline".to_string()),
            }
        }

        fn download(
            &self,
            _asset_name: &str,
            progress: &mut dyn FnMut(u64, u64),
        ) -> Result<Vec<u8>, String> {
            let total = self.asset.len() as u64;
            progress(total, total);
            Ok(self.asset.clone())
        }
    }

    struct Fixture {
        secret: String,
        public: String,
        exe: PathBuf,
        _dir: tempfile::TempDir,
    }

    impl Fixture {
        fn new() -> Self {
            let (secret, public) = generate_keypair();
            let dir = tempfile::tempdir().unwrap();
            let exe = dir.path().join("clouddeck");
            fs::write(&exe, b"old").unwrap();
            Self {
                secret,
                public,
                exe,
                _dir: dir,
            }
        }

        fn updater(
            &self,
            cx: &mut TestAppContext,
            serving: Option<&str>,
            asset: &[u8],
        ) -> Entity<Updater> {
            let source = Arc::new(FakeSource {
                release: serving.map(|version| signed_manifest(&self.secret, version, asset)),
                asset: asset.to_vec(),
            });
            cx.new(|_| {
                Updater::new(
                    source,
                    self.public.clone(),
                    Version::parse("0.2.0").unwrap(),
                    Some(Target::LinuxX86_64),
                    self.exe.clone(),
                )
            })
        }
    }

    fn signed_manifest(secret: &str, version: &str, asset: &[u8]) -> (Vec<u8>, String) {
        let manifest = Manifest {
            version: version.to_string(),
            published_at: "2026-10-24T00:00:00Z".to_string(),
            notes: "Faster lists".to_string(),
            assets: BTreeMap::from([(
                Target::LinuxX86_64.id().to_string(),
                Asset {
                    name: format!("CloudDeck-{version}-linux-x86_64.tar.gz"),
                    sha256: sha256_hex(asset),
                    size: asset.len() as u64,
                },
            )]),
        };
        let manifest_bytes = to_canonical_json(&manifest);
        let signature = sign_bytes(&manifest_bytes, secret).unwrap();
        (manifest_bytes, signature)
    }

    fn check(cx: &mut TestAppContext, updater: &Entity<Updater>) {
        updater.update(cx, |updater, cx| updater.check(cx));
        cx.run_until_parked();
    }

    fn open(cx: &mut TestAppContext, updater: &Entity<Updater>) -> VisualTestContext {
        let handle = cx
            .update(|cx| {
                gpui_component::init(cx);
                open_window(cx, updater)
            })
            .expect("update window opens");
        let window_cx = VisualTestContext::from_window(handle.into(), cx);
        window_cx.run_until_parked();
        window_cx
    }

    fn shown(cx: &mut VisualTestContext, selector: &'static str) -> bool {
        cx.debug_bounds(selector).is_some()
    }

    fn click(cx: &mut VisualTestContext, selector: &'static str) {
        let bounds = cx
            .debug_bounds(selector)
            .unwrap_or_else(|| panic!("{selector} should be rendered"));
        cx.simulate_click(bounds.center(), Default::default());
    }

    #[gpui::test]
    fn available_shows_title_notes_later_and_install(cx: &mut TestAppContext) {
        let fixture = Fixture::new();
        let updater = fixture.updater(cx, Some("0.3.0"), ASSET);
        check(cx, &updater);

        let mut window = open(cx, &updater);

        assert!(shown(&mut window, "update.title"));
        assert!(shown(&mut window, "update.notes"));
        assert!(shown(&mut window, "update.later"));
        assert!(shown(&mut window, "update.install"));
    }

    #[gpui::test]
    fn up_to_date_offers_only_ok(cx: &mut TestAppContext) {
        let fixture = Fixture::new();
        let updater = fixture.updater(cx, Some("0.2.0"), ASSET);
        check(cx, &updater);

        let mut window = open(cx, &updater);

        assert!(shown(&mut window, "update.ok"));
        assert!(!shown(&mut window, "update.install"));
    }

    #[gpui::test]
    fn failure_shows_the_error(cx: &mut TestAppContext) {
        let fixture = Fixture::new();
        let updater = fixture.updater(cx, None, ASSET);
        check(cx, &updater);

        let mut window = open(cx, &updater);

        assert!(shown(&mut window, "update.error"));
        assert!(shown(&mut window, "update.ok"));
    }

    #[cfg(target_os = "linux")]
    #[gpui::test]
    fn install_click_ends_with_relaunch_offered(cx: &mut TestAppContext) {
        let fixture = Fixture::new();
        let tar_gz = tar_gz_with_binary(b"new");
        let updater = fixture.updater(cx, Some("0.3.0"), &tar_gz);
        check(cx, &updater);
        let mut window = open(cx, &updater);

        click(&mut window, "update.install");
        cx.run_until_parked();

        let state = updater.read_with(cx, |updater, _| updater.state().clone());
        assert!(
            matches!(state, UpdateState::ReadyToRelaunch { .. }),
            "{state:?}"
        );
        assert!(shown(&mut window, "update.relaunch"));
    }

    #[gpui::test]
    fn auto_check_that_is_up_to_date_does_not_open_a_window(cx: &mut TestAppContext) {
        let fixture = Fixture::new();
        let updater = fixture.updater(cx, Some("0.2.0"), ASSET);
        cx.update(|cx| register(cx, &updater));

        updater.update(cx, |updater, cx| updater.maybe_auto_check(true, cx));
        cx.run_until_parked();

        assert_eq!(cx.update(|cx| cx.windows().len()), 0);
    }

    #[gpui::test]
    fn auto_check_that_finds_an_update_opens_a_window(cx: &mut TestAppContext) {
        let fixture = Fixture::new();
        let updater = fixture.updater(cx, Some("0.3.0"), ASSET);
        cx.update(|cx| {
            gpui_component::init(cx);
            register(cx, &updater)
        });

        updater.update(cx, |updater, cx| updater.maybe_auto_check(true, cx));
        cx.run_until_parked();

        assert_eq!(cx.update(|cx| cx.windows().len()), 1);
    }

    #[cfg(target_os = "linux")]
    fn tar_gz_with_binary(content: &[u8]) -> Vec<u8> {
        let mut builder = tar::Builder::new(flate2::write::GzEncoder::new(
            Vec::new(),
            flate2::Compression::default(),
        ));
        let mut header = tar::Header::new_gnu();
        header.set_path("clouddeck").unwrap();
        header.set_size(content.len() as u64);
        header.set_mode(0o755);
        header.set_cksum();
        builder.append(&header, content).unwrap();
        builder.into_inner().unwrap().finish().unwrap()
    }
}
