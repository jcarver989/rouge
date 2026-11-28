# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.7.0](https://github.com/jcarver989/rouge/compare/rouge-webview-v0.6.0...rouge-webview-v0.7.0) - 2026-11-28

### Added

- *(webview)* fade a page with page_opacity
- *(webview)* add exclusive web views and pages laid out at a fixed size
- *(webview)* embed system web views placed by layout
- support host-styled widgets, optional grammars, and web size budgets
- *(framer)* [**breaking**] stop generating PUBLISHING.md
- *(framer)* add Rouge app publishing preset
- *(web)* fall back to WebGL2 when WebGPU is unavailable

### Fixed

- *(ci)* install WebKitGTK and run webview platform tests under nextest

### Other

- *(webview)* describe web views, their engines and their limits
- describe the mobile hosts, the editor contract and accessible text editing
- describe accessibility support and remaining platform gaps
- migrate workspace tasks from Just to Mise
- describe mobile preview publishing and store promotion
- show and describe the mobile widgets
- describe adaptive widgets and the updated design system
- unify example launch commands and namespace Just recipes
- describe Android support and the platform traits
- prepare crates for publishing to crates.io
- *(widgets)* fold rouge-picker into rouge-widgets
- relicense under MIT and add crate publishing metadata
- remove cross-crate development dependency cycles
- add a README
