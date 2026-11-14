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

## Onboarding (providers)

Rendered from `design/onboarding.html` with `design/onboarding-shots.json`. Window shots are 1.5x (1000pt wide becomes 1500px); the `6m`/`6n` detail crops are 2x. These are all committed: they are the build spec for the onboarding window.

| Image | What it shows |
|---|---|
| ![61-onboarding-aws-ready](61-onboarding-aws-ready.png) `61-onboarding-aws-ready.png` | Onboarding, AWS selected: provider rail with per-provider status lines, profile list (first row selected), SSO badges (signed out, expires in 6 h, couldn't check sign-in), Sign in with SSO button, footer with credentials note and Open button. |
| ![62-onboarding-aws-detecting](62-onboarding-aws-detecting.png) `62-onboarding-aws-detecting.png` | AWS, looking for profiles: skeleton rows and a spinner line; rail says Looking…; Cancel in the footer. |
| ![63-onboarding-aws-nothing-found](63-onboarding-aws-nothing-found.png) `63-onboarding-aws-nothing-found.png` | AWS, no profiles in ~/.aws/config: empty state with a copyable 'aws configure sso' command, Check again, Choose config folder…, and an Explore demo fleet link. |
| ![64-onboarding-aws-sso-sign-in](64-onboarding-aws-sso-sign-in.png) `64-onboarding-aws-sso-sign-in.png` | AWS SSO sign-in inline in the pane (AuthStep::DeviceCode): device code, start URL and expiry, Open browser again, Copy code, waiting spinner; rail says Signing in…. |
| ![65-onboarding-aws-region-picker](65-onboarding-aws-region-picker.png) `65-onboarding-aws-region-picker.png` | AWS scope step after opening a profile: region checklist with last-known instance counts, Select all, Back to profiles, 'n of 6 selected' and Load n regions. |
| ![66-onboarding-aws-config-error](66-onboarding-aws-config-error.png) `66-onboarding-aws-config-error.png` | AWS config file unreadable: parse error with file and line, Check again, Open file, Copy error; rail shows a red Config error dot. |
| ![67-onboarding-gcp-ready](67-onboarding-gcp-ready.png) `67-onboarding-gcp-ready.png` | Google Cloud selected: signed-in account and ADC file path above the list, project rows (name, project id, zones last used), Open. |
| ![68-onboarding-gcp-no-adc](68-onboarding-gcp-no-adc.png) `68-onboarding-gcp-no-adc.png` | Google Cloud with no Application Default Credentials: copyable 'gcloud auth application-default login', Check again, Choose config folder…, where CloudDeck looked. |
| ![69-onboarding-gcp-service-account](69-onboarding-gcp-service-account.png) `69-onboarding-gcp-service-account.png` | Google Cloud ADC file is a service_account key: warning that it is not supported yet and the user-login command. |
| ![6a-onboarding-gcp-zone-picker](6a-onboarding-gcp-zone-picker.png) `6a-onboarding-gcp-zone-picker.png` | Google Cloud scope step: zone tree with regions as parents (checked, mixed, unchecked), zone children indented, 'n of m zones' per region, Load n zones. |
| ![6b-onboarding-extension-ready](6b-onboarding-extension-ready.png) `6b-onboarding-extension-ready.png` | Extension provider (Fake Cloud): connections from connections(), one signed out; Connect… button in the header. |
| ![6c-onboarding-extension-methods](6c-onboarding-extension-methods.png) `6c-onboarding-extension-methods.png` | Extension sign-in, choose method: one card per AuthMethod (label, description). |
| ![6d-onboarding-extension-prompt](6d-onboarding-extension-prompt.png) `6d-onboarding-extension-prompt.png` | Extension sign-in, AuthStep::Prompt: generated form, secret field masked, hint line, Connect. |
| ![6e-onboarding-extension-browser](6e-onboarding-extension-browser.png) `6e-onboarding-extension-browser.png` | Extension sign-in, AuthStep::OpenUrl: Continue in your browser, waiting spinner, Open again. |
| ![6f-onboarding-extension-disabled](6f-onboarding-extension-disabled.png) `6f-onboarding-extension-disabled.png` | Installed but disabled extension: greyed rail item, pane explains it is not enabled, Manage extensions…. |
| ![6g-onboarding-demo](6g-onboarding-demo.png) `6g-onboarding-demo.png` | Demo fleet selected: demo card, Open demo fleet, DEMO tag note. |
| ![6h-onboarding-add-provider](6h-onboarding-add-provider.png) `6h-onboarding-add-provider.png` | Add provider…: extensions explanation, copyable install command, Manage extensions…. |
| ![6i-onboarding-opening](6i-onboarding-opening.png) `6i-onboarding-opening.png` | After Load/Open: confirmation that the main window opens and onboarding closes for good. |
| ![6j-onboarding-aws-ready-dark](6j-onboarding-aws-ready-dark.png) `6j-onboarding-aws-ready-dark.png` | Dark theme, same state as 61. |
| ![6k-onboarding-gcp-zone-picker-dark](6k-onboarding-gcp-zone-picker-dark.png) `6k-onboarding-gcp-zone-picker-dark.png` | Dark theme, same state as 6a. |
| ![6m-onboarding-detail-rail](6m-onboarding-detail-rail.png) `6m-onboarding-detail-rail.png` | Detail, 2x: the provider rail. Brand block, PROVIDERS header, provider rows (30px tile, name 13/600, status line 11 with coloured dot), selected and disabled rows, dashed Add provider…, Manage extensions… at the bottom. |
| ![6n-onboarding-detail-connection-rows](6n-onboarding-detail-connection-rows.png) `6n-onboarding-detail-connection-rows.png` | Detail, 2x: connection rows. 34px initials badge (accent when selected), name 13/600, detail 11 dim, right-aligned auth status with dot and SSO pill. |
