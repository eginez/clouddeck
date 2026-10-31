# Design

Visual specification for the CloudDeck main window redesign ("Mac-native fleet UI, Zed-style"). It is a reference for building the gpui UI, not shipped code: nothing in here is imported by any crate.

This folder is the visual source of truth for layout, copy, states and behaviour. Product doctrine (what CloudDeck is for and what to leave out) is in [`brain/domains/product.md`](../brain/domains/product.md).

## Files

| Path | What it is | Who reads it |
|---|---|---|
| [`main-window.html`](main-window.html) | The interactive mockup. One self-contained page (HTML, CSS and JavaScript, no dependencies) with sample data: 20 instances across 4 regions. Open it in a browser and click through. | People. It needs JavaScript, so models should use the images instead. |
| [`images/`](images/) | PNG renders of the mockup, one per state or part, plus [`images/README.md`](images/README.md), which lists the committed images and what they show. Only the images the root README and `scripts/render-all.sh` use are committed; `render.swift` regenerates the rest. | Models and people. Start here if you cannot run the page. |
| [`icon/`](icon/) | The app icon: master SVG, PNG exports, the macOS `AppIcon.icns`, a generator script and a size board. See [`icon/README.md`](icon/README.md). | People and the app bundle. |
| [`shots.json`](shots.json) | The manifest the renderer reads: for each image, its name, description, the JavaScript that puts the mockup in that state, the element to capture, and the scale. | The renderer. Edit it to add or change an image. |
| [`render.swift`](render.swift) | The renderer. A small Swift script that loads `main-window.html` in the WebKit built into macOS, applies each shot's setup, and writes the PNGs. No downloads or browser install. macOS only. | Whoever regenerates the images. |

## The mockup (`main-window.html`)

Controls at the top of the page:
- **Light / Dark** switches the theme.
- **Live data / Demo data** turns the DEMO tag in the status bar on and off.
- **Wide / Medium / Narrow / Compact / Short** resize the main window to show the responsive behaviour.

What to try in the main window:
- Click rows and column headers. ↑/↓ or j/k move the selection, and ⌘F or `/` focuses search.
- ⌘P switches profile, ⌘⇧R switches region (multi-select), ⌘⇧P opens the command palette, ⌘B and ⌘⌥I toggle the sidebar and inspector, and ⌘C and ⌘O copy and open the selected instance.
- Type in search: `state:stopped`, `owner:esteban`, `age:>30d`, `-has:public-ip`, `type:t3.*`. Autocomplete and error hints appear as you type; clicking a sidebar facet edits the query.
- Click the amber "eu-central-1 failed" item in the status bar, then Retry region.
- Right-click a row for the context menu.

Below the main window are the Settings window, the first-run welcome window and the state cards (failures, offline, signed out, demo, empty fleet, loading, error, no match, search syntax, keyboard shortcuts).

What is fake: the data, the profiles and regions, the account IDs, and every action. Copy and Open in Console only show feedback. Instance fields like AMI, key pair and state reason are not in the core model yet.

## Regenerating the images

Run from the repository root, on a Mac with Xcode or the Command Line Tools:

```bash
swiftc -swift-version 5 -o /tmp/clouddeck-render design/render.swift
/tmp/clouddeck-render design/shots.json design/main-window.html design/images
```

Add a name prefix as a fourth argument to render only some shots, for example `... design/images 3` for everything whose name starts with `3`. A full run takes about 30 seconds and rewrites all PNGs. Window shots are 1.5x (1280pt wide becomes 1920px) and state cards 2x.

To add a shot, append an entry to `shots.json`: `setup` is JavaScript that runs after the page loads and may use `await`, `clip` is a CSS selector for the element to capture, and the helpers `A()`, `set()`, `pick()`, `key()`, `dark()`, `size()` and `wait()` are defined in `prelude`. Then re-run the renderer. `images/README.md` is not written by `render.swift`, so add the new image's row to its table by hand.

## Keeping it in sync

- Change `main-window.html` first, then re-render. Do not edit PNGs by hand.
- When the design changes, update the mockup and re-render the affected images in the same change.
