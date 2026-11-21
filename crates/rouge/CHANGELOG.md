# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.3.0](https://github.com/jcarver989/rouge/compare/rouge-v0.2.0...rouge-v0.3.0) - 2026-11-21

### Added

- *(core)* expose the keyboard inset and the window scope
- *(widgets)* [**breaking**] add a Stop control for busy composers
- *(framer)* add Rouge app publishing preset

### Fixed

- *(ios)* follow the system appearance

### Other

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
