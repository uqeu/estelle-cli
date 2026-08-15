# Terminal UI research

Checked 2026-08-04. Firecrawl reported **6,963 credits remaining** after the
initial six scrapes. The GitHub captures proved the scrape path but mostly
showed GitHub chrome, so Codex, Kimi, JCode, and Goose are evaluated from their
local vendor source. Ratatui and Sentry are evaluated from rendered product
pages captured through Firecrawl.

## Codex

Codex keeps durable chrome at the terminal edges: transcript in the flexible
middle and a composer with a pre-allocated popup budget at the bottom. Focus is
not inferred from colour; the active popup owns Up/Down/Esc/Enter and only the
focused textarea emits a cursor. An empty composer remains an obvious input
surface rather than disappearing. Estelle should keep that geometry and input
ownership, but not copy Codex's wording, palette, or oversized list density.
Source: `vendor-reference/codex/codex-rs/tui/src/bottom_pane/chat_composer.rs:930`
and `:2020`; `popup_consts.rs:11`.

## Kimi CLI

Kimi anchors slash suggestions to the composer cursor and caps the visible
menu, so the command surface is close to the action and never becomes a second
page. Focus is explicit because suggestions exist only while the input owns
focus; task-browser navigation has a separate key owner. Empty states retain
the composer and a useful next action. Estelle should not copy Kimi's blue/cyan
identity or its exact menu styling; the reusable principle is one bottom anchor
and one navigation owner. Source:
`vendor-reference/kimi-cli/src/kimi_cli/ui/shell/prompt.py:343` and `:1545`;
`vendor-reference/kimi-cli/src/kimi_cli/ui/shell/task_browser.py:321`.

## JCode

JCode treats suggestions as a late overlay, so opening them does not move the
input rows. Its optional right rail uses a single separating border whose style
changes with focus, and it prevents ambiguous triple splits while capping swarm
width. Empty or unavailable surfaces are structurally absent rather than
stacked beneath another view. Estelle should not reproduce its exact compact
chrome or terminology; it should take the visible focus boundary and strict
one-auxiliary-surface rule. Source:
`vendor-reference/jcode/crates/jcode-tui/src/tui/ui_input.rs:2558`;
`vendor-reference/jcode/crates/jcode-tui-render/src/chrome.rs:31`;
`vendor-reference/jcode/crates/jcode-tui/src/tui/ui.rs:2727`.

## Goose

Goose separates protocol/session state from presentation and exposes action
requirements as explicit states rather than treating every stopped operation
as success. Its useful chrome principle is that approvals and required input
interrupt at a stable interaction boundary, with the underlying work remaining
legible. Focus follows the action that currently requires a decision. Empty
operations say that input is needed instead of displaying a decorative zero.
Estelle must not copy Goose's product vocabulary or turn ACP into an auth door;
ACP remains interoperability while Estelle credentials stay server-owned.
Source: `vendor-reference/goose/crates/goose/src/action_required_manager.rs:271`
and `vendor-reference/goose/crates/goose/src/session`.

## Ratatui showcase

The Ratatui showcase demonstrates predictable edge chrome: persistent left
navigation, top search/actions, a bounded content column, and an optional right
outline. Focus is shown by a high-contrast selected row, not layout movement.
The small showcase index has three direct choices rather than filling absence
with ornament. Estelle should not copy this documentation-site card layout or
its blue visual identity; the useful lesson is stable regions and obvious
selection at every width. Rendered source: `https://ratatui.rs/showcase/`.

## Sentry Issues

Sentry places scope and filters above a dense issue table, persistent product
navigation at the left, and row-level status/actions at the point of use. Focus
and selection are visible through a selected navigation item, explicit filter
chips, and row controls. Empty or filtered states retain scope and the action
needed to change the result; issue rows name type, project, recency, trend,
events, users, priority, and assignee instead of presenting anonymous counts.
Estelle should not copy Sentry's web-dashboard density or purple styling; it
should preserve the information hierarchy and require app/org identity before
claiming production health. Rendered source:
`https://docs.sentry.io/product/issues/`.

## Estelle rules extracted

1. Composer and transient command surfaces share one bottom anchor.
2. Exactly one surface owns navigation, and its border/cursor states that fact.
3. The primary transcript and at most one auxiliary pane may paint at once.
4. Empty means a truthful reason plus the next available action.
5. Selection changes styling, never geometry.
6. Product identity comes from Estelle's cream, black, white, one earned red,
   and lily meadow rather than copied reference colours.
