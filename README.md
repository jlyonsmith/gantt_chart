# Gantt Chart Generator

[![coverage](https://shields.io/endpoint?url=https://raw.githubusercontent.com/jlyonsmith/gantt_chart/main/coverage.json)](https://github.com/jlyonsmith/gantt_chart/blob/main/coverage.json)
[![Crates.io](https://img.shields.io/crates/v/gantt_chart.svg)](https://crates.io/crates/gantt_chart)
[![Docs.rs](https://docs.rs/gantt_chart/badge.svg)](https://docs.rs/gantt_chart)

*New in v2.0, the tool now generates SVG files.*

This is a tool to generate simple Gantt charts. Here's some sample output:

![Gantt Chart Output](example/project.svg)

The focus of the tool is the generation of the chart from existing data and not the calculation of project dependencies.

Install with `cargo install gantt_chart`.  Run with `gantt-chart`.

It has the following features:

- Takes input date in a simple [JSON5](https://json5.org/) format
- Groups tasks by resource
- Schedules a tasks for each resource as soon as the previous one is complete
- Allows the creation of zero length project milestones
- Automatically generates resources colors using a Golden ration algorithm
- Customizable column widths
- Easy conversion to PNG or other formats using [resvg](https://crates.io/crates/resvg)
- Tasks can be shown as done or not-done
- You can add a dotted line to mark the current or other date

You can use the tool to quickly generate high level project timelines.  For full blown Gantt functionality, I recommend a tool like [OmniPlan](https://www.omnigroup.com/omniplan).
