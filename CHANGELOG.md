# Changelog

All notable changes to TradeTerminal will be documented in this file.

## [Unreleased]

### Added
- Initial Genie framework setup
- Product documentation (mission, tech-stack, roadmap)
- Minimal package.json for infrastructure
- Pre-commit and pre-push hooks
- **Trailing Stop with ratio-based triggers** - Support for fractional ratio triggers (e.g., `1/3`)
  - CLI command: `:ts 1/3 2%` - activate TS at 3x SL distance, 2% callback
  - Automatic calculation of trigger price based on SL-to-entry distance ratio
  - Formula: `trigger = entry ± (|SL - entry| × denominator/numerator)`
  - Example: Entry=100, SL=101 (1% risk), ratio=1/3 → trigger=103 (activate at 3% from entry, 3x risk distance)
  - Ratio parsing in CLI: `1/3`, `1/2`, `2/5`, etc.
  - Also supports API usage for ratio-based callbacks (less common)

### Changed
- Configured Genie with CLAUDE_CODE executor
- Disabled strict commit-advisory for regular development
- **BREAKING**: `TrailingStopRequest.trailing_stop` renamed to `TrailingStopRequest.target` (now uses `TrailingStopTarget` enum)
- **BREAKING**: `AutostopParams.trailing_stop` renamed to `AutostopParams.trailing_stop_target`

### Fixed
- Fixed broken @ reference in upgrade-genie.md spell
