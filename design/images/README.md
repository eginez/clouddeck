# Design images

PNG renders of the CloudDeck main-window design, for models and people that cannot run `design/main-window.html` (it needs JavaScript).
Each image is one state of the mockup. Only the images the root README and `scripts/render-all.sh` use are committed; the rest of the set is regenerated from `shots.json`. See `design/README.md` for how the mockup, the manifest and the renderer fit together.

Regenerate after changing `main-window.html` or `shots.json` (macOS only, uses the built-in WebKit, nothing to install):

```bash
swiftc -swift-version 5 -o /tmp/clouddeck-render design/render.swift
/tmp/clouddeck-render design/shots.json design/main-window.html design/images        # all
/tmp/clouddeck-render design/shots.json design/main-window.html design/images 3      # only names starting with 3
```

Window shots are 1.5x (1280pt wide = 1920px), state cards 2x. Light theme unless the name says dark.

| Image | What it shows |
|---|---|
| ![01-main-window-light](01-main-window-light.png) `01-main-window-light.png` | Main window, light, 20 instances, one row selected, inspector open, all regions, one region failing (status bar item). |
| ![02-main-window-dark](02-main-window-dark.png) `02-main-window-dark.png` | Main window, dark theme, same state as 01. |
| ![03-main-window-demo](03-main-window-demo.png) `03-main-window-demo.png` | Main window with the DEMO tag in the status bar. |
| ![04-main-window-no-selection](04-main-window-no-selection.png) `04-main-window-no-selection.png` | No row selected: inspector hidden, table full width. |
| ![23-query-autocomplete-values](23-query-autocomplete-values.png) `23-query-autocomplete-values.png` | Typing 'state:': values with live counts. |
| ![25-query-error-unknown-field](25-query-error-unknown-field.png) `25-query-error-unknown-field.png` | Unknown field: wavy underline and hint, searched as text. |
| ![27-search-empty-recent](27-search-empty-recent.png) `27-search-empty-recent.png` | Focused empty search: Recent searches and a one-line syntax reminder. |
| ![30-picker-profile](30-picker-profile.png) `30-picker-profile.png` | Profile picker popover from the status bar: search, rows with badge/account/region, Connect to AWS. |
| ![31-picker-region-multi](31-picker-region-multi.png) `31-picker-region-multi.png` | Region picker, multi-select: checkboxes, 'only' link on the highlighted row, counts; two regions selected (us-west-2, eu-central-1), status bar says '2 regions'. |
| ![32-failure-popover](32-failure-popover.png) `32-failure-popover.png` | Partial failure: amber status item opened, details popover with Copy error and Retry region. |
| ![33-command-palette](33-command-palette.png) `33-command-palette.png` | Command palette (⌘⇧P) with all commands. |
| ![36-context-menu](36-context-menu.png) `36-context-menu.png` | Row context menu (copy ID/IPs, open in console). |
| ![40-responsive-medium-960](40-responsive-medium-960.png) `40-responsive-medium-960.png` | Medium window 960pt wide: Launched column dropped. |
| ![41-responsive-narrow-700](41-responsive-narrow-700.png) `41-responsive-narrow-700.png` | Narrow 700pt: sidebar and inspector closed, only Name/ID/State/Type columns. |
| ![42-responsive-narrow-inspector-sheet](42-responsive-narrow-inspector-sheet.png) `42-responsive-narrow-inspector-sheet.png` | Narrow 700pt with a row selected: inspector as a sheet over a scrim. |
| ![44-responsive-compact-520](44-responsive-compact-520.png) `44-responsive-compact-520.png` | Compact 520pt: search collapsed to an icon, title hidden, truncated status bar buttons. |
| ![45-responsive-short-480](45-responsive-short-480.png) `45-responsive-short-480.png` | Short window 480pt tall: scrolling regions, same layout. |
| ![60-welcome-window](60-welcome-window.png) `60-welcome-window.png` | First-run welcome window: profile search list, connect actions. |
| ![70-state-several-regions-failed](70-state-several-regions-failed.png) `70-state-several-regions-failed.png` | State card: several regions failed (with the status bar strip it would show). |
| ![75-state-empty-fleet](75-state-empty-fleet.png) `75-state-empty-fleet.png` | State card: empty fleet (with the status bar strip it would show). |
| ![78-state-error](78-state-error.png) `78-state-error.png` | State card: error (with the status bar strip it would show). |
