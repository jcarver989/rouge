# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.7.0](https://github.com/jcarver989/rouge/compare/rouge-core-v0.6.0...rouge-core-v0.7.0) - 2026-11-28

### Added

- *(core)* track window regions that rouge cannot draw over
- *(core)* read a motion with value() to drive layout on every frame
- *(core)* let tests wait for the ui to go idle
- *(core)* [**breaking**] report an element's frame and corners in its paint region

### Other

- *(webview)* describe web views, their engines and their limits
- *(core)* [**breaking**] cache the paint of each child of a large container
- *(text)* reuse fitting label layouts and skip undecorated spans
- *(core)* reconcile keyed fragment removals and reorders in bulk

## [0.6.0](https://github.com/jcarver989/rouge/compare/rouge-core-v0.5.0...rouge-core-v0.6.0) - 2026-11-25

### Added

- *(widgets)* handle overlay collisions at window edges
- support host-styled widgets, optional grammars, and web size budgets

### Fixed

- *(render)* snap shared row edges in device space

## [0.5.0](https://github.com/jcarver989/rouge/compare/rouge-core-v0.4.1...rouge-core-v0.5.0) - 2026-11-24

### Added

- *(text)* [**breaking**] truncate text with an ellipsis

### Fixed

- *(editor)* truncate a placeholder that outgrows its editor

## [0.4.1](https://github.com/jcarver989/rouge/compare/rouge-core-v0.4.0...rouge-core-v0.4.1) - 2026-11-24

### Added

- *(editor)* support styled spans in editable text

### Fixed

- *(editor)* preserve tabs in multiline input

## [0.4.0](https://github.com/jcarver989/rouge/compare/rouge-core-v0.3.0...rouge-core-v0.4.0) - 2026-11-23

### Added

- *(android)* run on a GameActivity host with IME text input and accessibility
- *(core)* expose editable text, caret and selection to assistive technology
- *(core)* expose the focused editor and atomic edit transactions
- *(testing)* query accessible nodes and dispatch assistive actions
- *(core)* project element semantics into incremental AccessKit updates

### Other

- describe the mobile hosts, the editor contract and accessible text editing
- *(core)* key virtual list rows by model identity without enumerating keys
- describe accessibility support and remaining platform gaps
- add AccessKit dependencies for tree consumers and native hosts

## [0.3.0](https://github.com/jcarver989/rouge/compare/rouge-core-v0.2.0...rouge-core-v0.3.0) - 2026-11-21

### Added

- *(framer)* [**breaking**] stop generating PUBLISHING.md
- *(core)* make layout_transition a reactive prop
- *(core)* expose the keyboard inset and the window scope
- *(framer)* add Rouge app publishing preset

### Other

- migrate workspace tasks from Just to Mise
- describe mobile preview publishing and store promotion
- show and describe the mobile widgets

## [0.2.0](https://github.com/jcarver989/rouge/compare/rouge-core-v0.1.2...rouge-core-v0.2.0) - 2026-11-19

### Added

- *(widgets)* add reactive grid placement and centered reading cells
- *(web)* fall back to WebGL2 when WebGPU is unavailable
- *(widgets)* [**breaking**] add responsive sheets and richer control interactions
- *(core)* support scrolling underlays in virtual lists
- *(core)* add grid column spans and named bounds inspection
- *(core)* [**breaking**] expose window presentation and improve touch gestures
- *(core)* map motion bindings and spring from release velocity
- *(core)* detach and reattach a window's render target

### Fixed

- *(rouge)* tighten the Android surface, JNI and packaging contracts

### Other

- describe adaptive widgets and the updated design system
- unify example launch commands and namespace Just recipes
- describe Android support and the platform traits

## [0.1.2](https://github.com/jcarver989/rouge/compare/rouge-core-v0.1.1...rouge-core-v0.1.2) - 2026-11-18

### Other

- Merge pull request #5 from jcarver989/ci/install-fontconfig

## [0.1.1](https://github.com/jcarver989/rouge/compare/rouge-core-v0.1.0...rouge-core-v0.1.1) - 2026-11-18

### Added

- *(core)* support text selection across virtual rows
- *(core)* report the renderer's memory from TestUi

### Fixed

- *(core)* bind virtual selections to their list and pointer gesture

### Other

- *(core)* cover rendering after shrinking and growing a window
