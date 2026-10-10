# Design images

PNG renders of every part of the CloudDeck main-window design, for models and people that cannot run `design/main-window.html` (it needs JavaScript).
Each image is one state of the mockup. See `design/README.md` for how the mockup, the manifest and the renderer fit together.

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
| ![10-inspector-running](10-inspector-running.png) `10-inspector-running.png` | Inspector for a running instance (summary line, instance, network, security groups, tags with collapse). |
| ![11-inspector-stopped](11-inspector-stopped.png) `11-inspector-stopped.png` | Inspector for a stopped instance: summary shows stop reason, public IP says 'Released while stopped'. |
| ![12-inspector-no-tags](12-inspector-no-tags.png) `12-inspector-no-tags.png` | Inspector for an instance with no tags (explanation, shows as unnamed). |
| ![13-inspector-stale-region](13-inspector-stale-region.png) `13-inspector-stale-region.png` | Inspector for an instance in a failed region: 'From the 12:01 refresh' note. |
| ![14-inspector-all-tags](14-inspector-all-tags.png) `14-inspector-all-tags.png` | Inspector with all 13 tags expanded ('Show fewer'). |
| ![20-sidebar-facets-filtered](20-sidebar-facets-filtered.png) `20-sidebar-facets-filtered.png` | Sidebar facets with live counts after clicking Stopped; query field shows state:stopped; toolbar 'N filter'. |
| ![21-query-highlight-and-filter](21-query-highlight-and-filter.png) `21-query-highlight-and-filter.png` | Query with several terms, highlighted, table filtered. |
| ![22-query-autocomplete-fields](22-query-autocomplete-fields.png) `22-query-autocomplete-fields.png` | Typing 'st': autocomplete suggests field names with hints. |
| ![23-query-autocomplete-values](23-query-autocomplete-values.png) `23-query-autocomplete-values.png` | Typing 'state:': values with live counts. |
| ![24-query-autocomplete-tags](24-query-autocomplete-tags.png) `24-query-autocomplete-tags.png` | Typing 'tag:Team=': tag values with counts. |
| ![25-query-error-unknown-field](25-query-error-unknown-field.png) `25-query-error-unknown-field.png` | Unknown field: wavy underline and hint, searched as text. |
| ![26-query-error-bad-age](26-query-error-bad-age.png) `26-query-error-bad-age.png` | Bad age value: hint explains the form. |
| ![27-search-empty-recent](27-search-empty-recent.png) `27-search-empty-recent.png` | Focused empty search: Recent searches and a one-line syntax reminder. |
| ![28-no-match](28-no-match.png) `28-no-match.png` | Query with no results: empty table message. |
| ![30-picker-profile](30-picker-profile.png) `30-picker-profile.png` | Profile picker popover from the status bar: search, rows with badge/account/region, Connect to AWS. |
| ![31-picker-region-multi](31-picker-region-multi.png) `31-picker-region-multi.png` | Region picker, multi-select: checkboxes, 'only' link on the highlighted row, counts; two regions selected (us-west-2, eu-central-1), status bar says '2 regions'. |
| ![32-failure-popover](32-failure-popover.png) `32-failure-popover.png` | Partial failure: amber status item opened, details popover with Copy error and Retry region. |
| ![33-command-palette](33-command-palette.png) `33-command-palette.png` | Command palette (⌘⇧P) with all commands. |
| ![34-command-palette-filtered](34-command-palette-filtered.png) `34-command-palette-filtered.png` | Palette filtered by typing 'copy'. |
| ![35-copy-feedback](35-copy-feedback.png) `35-copy-feedback.png` | Status bar flash after Copy ID ('Copied i-…'). |
| ![36-context-menu](36-context-menu.png) `36-context-menu.png` | Row context menu (copy ID/IPs, open in console). |
| ![40-responsive-medium-960](40-responsive-medium-960.png) `40-responsive-medium-960.png` | Medium window 960pt wide: Launched column dropped. |
| ![41-responsive-narrow-700](41-responsive-narrow-700.png) `41-responsive-narrow-700.png` | Narrow 700pt: sidebar and inspector closed, only Name/ID/State/Type columns. |
| ![42-responsive-narrow-inspector-sheet](42-responsive-narrow-inspector-sheet.png) `42-responsive-narrow-inspector-sheet.png` | Narrow 700pt with a row selected: inspector as a sheet over a scrim. |
| ![43-responsive-narrow-sidebar-overlay](43-responsive-narrow-sidebar-overlay.png) `43-responsive-narrow-sidebar-overlay.png` | Narrow 700pt with the sidebar opened as an overlay. |
| ![44-responsive-compact-520](44-responsive-compact-520.png) `44-responsive-compact-520.png` | Compact 520pt: search collapsed to an icon, title hidden, truncated status bar buttons. |
| ![45-responsive-short-480](45-responsive-short-480.png) `45-responsive-short-480.png` | Short window 480pt tall: scrolling regions, same layout. |
| ![50-settings-appearance](50-settings-appearance.png) `50-settings-appearance.png` | Settings window, Appearance category (light). |
| ![51-settings-fleet](51-settings-fleet.png) `51-settings-fleet.png` | Settings window, Fleet category (light). |
| ![52-settings-table](52-settings-table.png) `52-settings-table.png` | Settings window, Table category (light). |
| ![53-settings-search](53-settings-search.png) `53-settings-search.png` | Settings window, Search category (light). |
| ![54-settings-keymap](54-settings-keymap.png) `54-settings-keymap.png` | Settings window, Keymap category (light). |
| ![55-settings-aws-privacy](55-settings-aws-privacy.png) `55-settings-aws-privacy.png` | Settings window, AWS & Privacy category (light). |
| ![56-settings-advanced](56-settings-advanced.png) `56-settings-advanced.png` | Settings window, Advanced category (light). |
| ![57-settings-about](57-settings-about.png) `57-settings-about.png` | Settings window, About category (light). |
| ![58-settings-search-results](58-settings-search-results.png) `58-settings-search-results.png` | Settings search for 'refresh' (results grouped by category, highlighted). |
| ![59-settings-changed-values](59-settings-changed-values.png) `59-settings-changed-values.png` | Settings with changed values: blue bars, Reset links, dot on the category. |
| ![5a-settings-appearance-dark](5a-settings-appearance-dark.png) `5a-settings-appearance-dark.png` | Settings window in dark theme. |
| ![60-welcome-window](60-welcome-window.png) `60-welcome-window.png` | First-run welcome window: profile search list, connect actions. |
| ![70-state-several-regions-failed](70-state-several-regions-failed.png) `70-state-several-regions-failed.png` | State card: several regions failed (with the status bar strip it would show). |
| ![71-state-throttled](71-state-throttled.png) `71-state-throttled.png` | State card: throttled (with the status bar strip it would show). |
| ![72-state-offline](72-state-offline.png) `72-state-offline.png` | State card: offline (with the status bar strip it would show). |
| ![73-state-signed-out-sso-expired](73-state-signed-out-sso-expired.png) `73-state-signed-out-sso-expired.png` | State card: signed out sso expired (with the status bar strip it would show). |
| ![74-state-demo-mode](74-state-demo-mode.png) `74-state-demo-mode.png` | State card: demo mode (with the status bar strip it would show). |
| ![75-state-empty-fleet](75-state-empty-fleet.png) `75-state-empty-fleet.png` | State card: empty fleet (with the status bar strip it would show). |
| ![76-state-search-syntax-card](76-state-search-syntax-card.png) `76-state-search-syntax-card.png` | State card: search syntax card (with the status bar strip it would show). |
| ![77-state-loading](77-state-loading.png) `77-state-loading.png` | State card: loading (with the status bar strip it would show). |
| ![78-state-error](78-state-error.png) `78-state-error.png` | State card: error (with the status bar strip it would show). |
| ![79-state-filter-matches-nothing](79-state-filter-matches-nothing.png) `79-state-filter-matches-nothing.png` | State card: filter matches nothing (with the status bar strip it would show). |
| ![7a-state-keyboard-shortcuts](7a-state-keyboard-shortcuts.png) `7a-state-keyboard-shortcuts.png` | State card: keyboard shortcuts (with the status bar strip it would show). |
