# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.4.0](https://github.com/jcarver989/rouge/compare/rouge-v0.3.0...rouge-v0.4.0) - 2026-11-23

### Added

- *(android)* run on a GameActivity host with IME text input and accessibility
- *(ios)* edit text through a UITextInput responder
- *(core)* expose editable text, caret and selection to assistive technology
- *(widgets)* pass input hints from text_input to the platform keyboard
- *(native)* route AccessKit events through each window’s winit adapter

### Other

- describe the mobile hosts, the editor contract and accessible text editing
- *(rouge)* split host plumbing out of the winit runner
- *(core)* key virtual list rows by model identity without enumerating keys
- describe accessibility support and remaining platform gaps
- *(gallery)* require every interactive control to have a role and name
- add AccessKit dependencies for tree consumers and native hosts

## [0.3.0](https://github.com/jcarver989/rouge/compare/rouge-v0.2.0...rouge-v0.3.0) - 2026-11-21

### Added

- *(framer)* [**breaking**] stop generating PUBLISHING.md
- *(rouge)* set desktop app icons from PNGs
- *(core)* expose the keyboard inset and the window scope
- *(widgets)* [**breaking**] add a Stop control for busy composers
- *(framer)* add Rouge app publishing preset

### Fixed

- *(ios)* follow the system appearance

### Other

- migrate workspace tasks from Just to Mise
- describe mobile preview publishing and store promotion
- show and describe the mobile widgets

## [0.2.0](https://github.com/jcarver989/rouge/compare/rouge-v0.1.2...rouge-v0.2.0) - 2026-11-19

### Added

- *(web)* fall back to WebGL2 when WebGPU is unavailable
- *(examples)* refresh TodoMVC with animated rings and waves
- *(examples)* rebuild the component gallery with interactive demos
- *(widgets)* [**breaking**] add responsive sheets and richer control interactions
- *(theme)* refine control colors and improve accent contrast
- *(core)* [**breaking**] expose window presentation and improve touch gestures
- *(rouge)* add Android support

### Fixed

- *(rouge)* tighten the Android surface, JNI and packaging contracts

### Other

- describe adaptive widgets and the updated design system
- unify example launch commands and namespace Just recipes
- *(examples)* extract the shared snapshot camera
- describe Android support and the platform traits
- *(rouge)* [**breaking**] isolate platform code behind Platform traits

## [0.1.1](https://github.com/jcarver989/rouge/compare/rouge-v0.1.0...rouge-v0.1.1) - 2026-11-18

### Added

- *(bench)* replace the bench example with a headless rouge-bench crate
