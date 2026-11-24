# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.4.1](https://github.com/jcarver989/rouge/compare/rouge-widgets-v0.4.0...rouge-widgets-v0.4.1) - 2026-11-24

### Fixed

- Make popover not wrap

## [0.4.0](https://github.com/jcarver989/rouge/compare/rouge-widgets-v0.3.0...rouge-widgets-v0.4.0) - 2026-11-23

### Added

- *(widgets)* pass input hints from text_input to the platform keyboard
- *(widgets)* drive transcript and data table rows from keyed list order
- *(widgets)* label window chrome and announce dialogs and status
- *(widgets)* expose range values and share accessible step actions
- *(widgets)* expose roles and selection state for choices and collections
- *(widgets)* give editable controls explicit and inherited field names

### Other

- describe the mobile hosts, the editor contract and accessible text editing
- *(core)* key virtual list rows by model identity without enumerating keys
- describe accessibility support and remaining platform gaps

## [0.3.0](https://github.com/jcarver989/rouge/compare/rouge-widgets-v0.2.0...rouge-widgets-v0.3.0) - 2026-11-21

### Added

- *(framer)* [**breaking**] stop generating PUBLISHING.md
- *(widgets)* snap the picker highlight to a hovered row
- *(widgets)* adapt toasts and side panels to handhelds
- *(widgets)* add mobile navigation, lists, pickers and bottom chrome
- *(widgets)* size controls for touch and open context menus by long press
- *(widgets)* [**breaking**] add a Stop control for busy composers
- *(framer)* add Rouge app publishing preset

### Fixed

- *(widgets)* show Stop only when the composer can cancel

### Other

- migrate workspace tasks from Just to Mise
- describe mobile preview publishing and store promotion
- *(widgets)* replace the spinner helpers with one Spinner view
- show and describe the mobile widgets
- *(widgets)* share drag and overlay helpers

## [0.2.0](https://github.com/jcarver989/rouge/compare/rouge-widgets-v0.1.2...rouge-widgets-v0.2.0) - 2026-11-19

### Added

- *(widgets)* add a circular progress indicator
- *(widgets)* let inline editors start in editing mode
- *(widgets)* [**breaking**] allow custom picker footers with keyboard hints
- *(widgets)* add trailing content and icon-only list row actions
- *(widgets)* support labels and badges in segments
- *(widgets)* add hold-to-confirm icon buttons
- *(widgets)* add reactive grid placement and centered reading cells
- *(web)* fall back to WebGL2 when WebGPU is unavailable
- *(widgets)* add streaming transcripts and a message composer
- *(widgets)* add avatars, badges, breadcrumbs, and toasts
- *(widgets)* add virtualized data tables and reorderable lists
- *(widgets)* add validated fields and number, tag, and code inputs
- *(widgets)* add mobile tabs, pull to refresh, and animated indicators
- *(widgets)* [**breaking**] add responsive sheets and richer control interactions
- *(widgets)* add spacing and motion tokens, grid helpers, and materials

### Fixed

- *(widgets)* clip long current breadcrumb labels
- *(widgets)* align Markdown blocks and scrollbar thumbs to the spacing grid
- *(widgets)* restore missing arc commands in icon paths

### Other

- describe adaptive widgets and the updated design system
- unify example launch commands and namespace Just recipes
- *(widgets)* enforce spacing tokens and cover grid alignment
- describe Android support and the platform traits

## [0.1.2](https://github.com/jcarver989/rouge/compare/rouge-widgets-v0.1.1...rouge-widgets-v0.1.2) - 2026-11-18

### Other

- Merge pull request #5 from jcarver989/ci/install-fontconfig

## [0.1.1](https://github.com/jcarver989/rouge/compare/rouge-widgets-v0.1.0...rouge-widgets-v0.1.1) - 2026-11-18

### Added

- *(widgets)* make Markdown text selectable and copyable

### Fixed

- *(widgets)* keep rendered and copied Markdown text in sync
- *(core)* bind virtual selections to their list and pointer gesture
