# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.7.0](https://github.com/jcarver989/rouge/compare/rouge-framer-v0.6.0...rouge-framer-v0.7.0) - 2026-11-28

### Added

- *(webview)* fade a page with page_opacity
- *(framer)* generate the Android half of the web view engine

## [0.6.0](https://github.com/jcarver989/rouge/compare/rouge-framer-v0.5.0...rouge-framer-v0.6.0) - 2026-11-25

### Added

- support host-styled widgets, optional grammars, and web size budgets

### Fixed

- *(ci)* restore generated syntax checks and web packaging test

## [0.4.0](https://github.com/jcarver989/rouge/compare/rouge-framer-v0.3.0...rouge-framer-v0.4.0) - 2026-11-23

### Added

- *(framer)* package Android apps on a generated GameActivity

### Other

- *(framer)* turn the mobile smoke fixture into a form
- *(ios)* raise the deployment floor to iOS 17

## [0.3.0](https://github.com/jcarver989/rouge/compare/rouge-framer-v0.2.0...rouge-framer-v0.3.0) - 2026-11-21

### Added

- *(framer)* [**breaking**] stop generating PUBLISHING.md
- *(framer)* [**breaking**] generate a Mise task for the iOS simulator
- *(framer)* [**breaking**] build mobile apps from generated Xcode and Gradle projects
- *(framer)* run the app on the iOS Simulator
- *(framer)* make Linux and macOS independently selectable
- *(framer)* add Rouge app publishing preset

### Fixed

- *(framer)* verify desktop release artifacts before upload
- *(framer)* isolate signing credentials from tool installation
- *(framer)* promote Pages state only after deployment

### Other

- *(framer)* add a mobile smoke fixture app
- *(framer)* remove unused publishing templates
- *(framer)* explain how to sign the first Google Play upload
