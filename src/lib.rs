/// Generate a Gantt chart
use chrono::{Datelike, Duration, NaiveDate};
use clap::Parser;
use core::fmt::Arguments;
use hypermelon::{attr::PathCommand::*, build, prelude::*};
use rand::prelude::*;
use serde::{Deserialize, Serialize};
use std::{
    error::Error,
    fs::File,
    io::{self, Error as IoError, Read, Write},
    path::PathBuf,
};

mod log_macros;

static GOLDEN_RATIO_CONJUGATE: f32 = 0.618033988749895;
static MONTH_NAMES: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

#[derive(Parser)]
#[clap(version, about, long_about = None)]
struct Cli {
    /// Specify the JSON data file
    #[arg(value_name = "INPUT_FILE")]
    input_file: Option<PathBuf>,

    /// The SVG output file
    #[arg(value_name = "OUTPUT_FILE")]
    output_file: Option<PathBuf>,

    /// The width of the item title column
    #[arg(value_name = "WIDTH", short, long, default_value_t = 210.0)]
    title_width: f32,

    /// The maximum width of each month
    #[arg(value_name = "WIDTH", short, long, default_value_t = 80.0)]
    max_month_width: f32,

    /// Add a resource table at the bottom of the graph
    #[arg(short, long, default_value_t = false)]
    add_resource_table: bool,
}

impl Cli {
    fn get_output(&self) -> Result<Box<dyn Write>, IoError> {
        match self.output_file {
            Some(ref path) => File::create(path).map(|f| Box::new(f) as Box<dyn Write>),
            None => Ok(Box::new(io::stdout())),
        }
    }

    fn get_input(&self) -> Result<Box<dyn Read>, IoError> {
        match self.input_file {
            Some(ref path) => File::open(path).map(|f| Box::new(f) as Box<dyn Read>),
            None => Ok(Box::new(io::stdin())),
        }
    }
}

pub trait GanttChartLog {
    fn output(self: &Self, args: Arguments);
    fn warning(self: &Self, args: Arguments);
    fn error(self: &Self, args: Arguments);
}

pub struct GanttChartTool<'a> {
    log: &'a dyn GanttChartLog,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct ItemData {
    pub title: String,
    pub duration: Option<i64>,
    #[serde(rename = "startDate", skip_serializing_if = "Option::is_none")]
    pub start_date: Option<NaiveDate>,
    #[serde(rename = "resource")]
    pub resource_index: Option<usize>,
    pub open: Option<bool>,
}

#[derive(Deserialize, Serialize, Debug)]
pub struct ChartData {
    pub title: String,
    #[serde(rename = "markedDate")]
    pub marked_date: Option<NaiveDate>,
    pub resources: Vec<String>,
    pub items: Vec<ItemData>,
}

#[derive(Debug)]
pub struct Gutter {
    left: f32,
    top: f32,
    right: f32,
    bottom: f32,
}

impl Gutter {
    pub fn height(&self) -> f32 {
        self.bottom + self.top
    }

    pub fn width(&self) -> f32 {
        self.right + self.left
    }
}

#[derive(Debug)]
struct RenderData {
    title: String,
    gutter: Gutter,
    row_gutter: Gutter,
    row_height: f32,
    resource_gutter: Gutter,
    resource_height: f32,
    marked_date_offset: Option<f32>,
    title_width: f32,
    max_month_width: f32,
    rect_corner_radius: f32,
    styles: Vec<String>,
    cols: Vec<ColumnRenderData>,
    rows: Vec<RowRenderData>,
    resources: Vec<String>,
}

#[derive(Debug)]
struct RowRenderData {
    title: String,
    resource_index: usize,
    offset: f32,
    // If length not present then this is a milestone
    length: Option<f32>,
    open: bool,
}

#[derive(Debug)]
struct ColumnRenderData {
    width: f32,
    month_name: String,
}

impl<'a> GanttChartTool<'a> {
    pub fn new(log: &'a dyn GanttChartLog) -> GanttChartTool {
        GanttChartTool { log }
    }

    pub fn run(
        self: &mut Self,
        args: impl IntoIterator<Item = std::ffi::OsString>,
    ) -> Result<(), Box<dyn Error>> {
        let cli = match Cli::try_parse_from(args) {
            Ok(cli) => cli,
            Err(err) => {
                output!(self.log, "{}", err.to_string());
                return Ok(());
            }
        };

        let chart_data = Self::read_chart_file(cli.get_input()?)?;
        let render_data =
            self.process_chart_data(cli.title_width, cli.max_month_width, &chart_data)?;
        let output = self.render_chart(cli.add_resource_table, &render_data)?;

        Self::write_svg_file(cli.get_output()?, &output)?;
        Ok(())
    }

    fn read_chart_file(mut reader: Box<dyn Read>) -> Result<ChartData, Box<dyn Error>> {
        let mut content = String::new();

        reader.read_to_string(&mut content)?;

        let chart_data: ChartData = json5::from_str(&content)?;

        Ok(chart_data)
    }

    fn write_svg_file(mut writer: Box<dyn Write>, output: &str) -> Result<(), Box<dyn Error>> {
        write!(writer, "{}", output)?;

        Ok(())
    }

    fn hsv_to_rgb(h: f32, s: f32, v: f32) -> u32 {
        let h_i = (h * 6.0) as usize;
        let f = h * 6.0 - h_i as f32;
        let p = v * (1.0 - s);
        let q = v * (1.0 - f * s);
        let t = v * (1.0 - (1.0 - f) * s);

        fn rgb(r: f32, g: f32, b: f32) -> u32 {
            ((r * 256.0) as u32) << 16 | ((g * 256.0) as u32) << 8 | ((b * 256.0) as u32)
        }

        if h_i == 0 {
            rgb(v, t, p)
        } else if h_i == 1 {
            rgb(q, v, p)
        } else if h_i == 2 {
            rgb(p, v, t)
        } else if h_i == 3 {
            rgb(p, q, v)
        } else if h_i == 4 {
            rgb(t, p, v)
        } else {
            rgb(v, p, q)
        }
    }

    fn process_chart_data(
        self: &Self,
        title_width: f32,
        max_month_width: f32,
        chart_data: &ChartData,
    ) -> Result<RenderData, Box<dyn Error>> {
        fn num_days_in_month(year: i32, month: u32) -> u32 {
            // the first day of the next month...
            let (y, m) = if month == 12 {
                (year + 1, 1)
            } else {
                (year, month + 1)
            };
            let d = NaiveDate::from_ymd(y, m, 1);

            // ...is preceded by the last day of the original month
            d.pred().day()
        }

        // TODO(john): Fail if only one task

        let mut start_date = NaiveDate::MAX;
        let mut end_date = NaiveDate::MIN;
        let mut date = NaiveDate::MIN;

        // Determine the project start & end dates
        for (i, item) in chart_data.items.iter().enumerate() {
            if let Some(item_start_date) = item.start_date {
                date = item_start_date;

                if item_start_date < start_date {
                    // TODO: Ensure the start date is not on a weekend
                    start_date = date;
                }
            } else if i == 0 {
                return Err(From::from(format!("First item must contain a start date")));
            }

            if let Some(item_days) = item.duration {
                // TODO(john): Be smarter about adding days and skip the weekends
                // TODO(john): Keep a "shadow" list of the _real_ durations that includes the weekends
                date += Duration::days(item_days);
            }

            if end_date < date {
                end_date = date;
            }

            if let Some(item_resource_index) = item.resource_index {
                if item_resource_index >= chart_data.resources.len() {
                    return Err(From::from(format!("Resource index is out of range")));
                }
            } else if i == 0 {
                return Err(From::from(format!(
                    "First item must contain a resource index"
                )));
            }
        }

        start_date = NaiveDate::from_ymd(start_date.year(), start_date.month(), 1);
        end_date = NaiveDate::from_ymd(
            end_date.year(),
            end_date.month(),
            num_days_in_month(end_date.year(), end_date.month()),
        );

        // Create all the column data
        let mut all_items_width: f32 = 0.0;
        let mut num_item_days: u32 = 0;
        let mut cols = vec![];

        date = start_date;

        while date <= end_date {
            let item_days = num_days_in_month(date.year(), date.month());
            let item_width = max_month_width * (item_days as f32) / 31.0;

            num_item_days += item_days;
            all_items_width += item_width;

            cols.push(ColumnRenderData {
                width: item_width,
                month_name: MONTH_NAMES[date.month() as usize - 1].to_string(),
            });

            date = NaiveDate::from_ymd(
                date.year() + (if date.month() == 12 { 1 } else { 0 }),
                date.month() % 12 + 1,
                1,
            );
        }

        date = start_date;

        let mut resource_index: usize = 0;
        let gutter = Gutter {
            left: 10.0,
            top: 80.0,
            right: 10.0,
            bottom: 10.0,
        };
        let row_gutter = Gutter {
            left: 5.0,
            top: 5.0,
            right: 5.0,
            bottom: 5.0,
        };
        // TODO(john): The 20.0 should be configurable, and for the resource table
        let row_height = row_gutter.height() + 20.0;
        let resource_gutter = Gutter {
            left: 10.0,
            top: 10.0,
            right: 10.0,
            bottom: 10.0,
        };
        let resource_height = resource_gutter.height() + 20.0;
        let mut rows = vec![];

        // Calculate the X offsets of all the bars and milestones
        for item in chart_data.items.iter() {
            if let Some(item_start_date) = item.start_date {
                date = item_start_date;
            }

            let offset = title_width
                + gutter.left
                + ((date - start_date).num_days() as f32) / (num_item_days as f32)
                    * all_items_width;

            let mut length: Option<f32> = None;

            if let Some(item_days) = item.duration {
                // TODO(john): Use the "shadow" duration instead of the actual duration (see comment above)
                date += Duration::days(item_days);
                length = Some((item_days as f32) / (num_item_days as f32) * all_items_width);
            }

            if let Some(item_resource_index) = item.resource_index {
                resource_index = item_resource_index;
            }

            rows.push(RowRenderData {
                title: item.title.clone(),
                resource_index,
                offset,
                length,
                open: item.open.unwrap_or(false),
            });
        }

        let marked_date_offset = if let Some(date) = chart_data.marked_date {
            // TODO(john): Put this offset calculation in a function
            Some(
                title_width
                    + gutter.left
                    + ((date - start_date).num_days() as f32) / (num_item_days as f32)
                        * all_items_width,
            )
        } else {
            None
        };

        let mut styles = vec![
            ".outer-lines{stroke-width:3;stroke:#aaaaaa;}".to_owned(),
            ".inner-lines{stroke-width:2;stroke:#dddddd;}".to_owned(),
            ".item{font-family:Arial;font-size:12pt;dominant-baseline:middle;}".to_owned(),
            ".resource{font-family:Arial;font-size:12pt;text-anchor:end;dominant-baseline:middle;}".to_owned(),
            ".title{font-family:Arial;font-size:18pt;}".to_owned(),
            ".heading{font-family:Arial;font-size:16pt;dominant-baseline:middle;text-anchor:middle;}".to_owned(),
            ".task-heading{dominant-baseline:middle;text-anchor:start;}".to_owned(),
            ".milestone{fill:black;stroke-width:1;stroke:black;}".to_owned(),
            ".marker{stroke-width:2;stroke:#888888;stroke-dasharray:7;}".to_owned(),
        ];

        // Generate random resource colors based on https://martin.ankerl.com/2009/12/09/how-to-create-random-colors-programmatically/
        let mut rng = rand::thread_rng();
        let mut h: f32 = rng.gen();

        for i in 0..chart_data.resources.len() {
            let rgb = GanttChartTool::hsv_to_rgb(h, 0.5, 0.5);

            styles.push(format!(
                ".resource-{}-closed{{fill:#{1:06x};stroke-width:1;stroke:#{1:06x};}}",
                i, rgb,
            ));
            styles.push(format!(
                ".resource-{}-open{{fill:none;stroke-width:2;stroke:#{1:06x};}}",
                i, rgb,
            ));

            h = (h + GOLDEN_RATIO_CONJUGATE) % 1.0;
        }

        Ok(RenderData {
            title: chart_data.title.to_owned(),
            gutter,
            row_gutter,
            row_height,
            resource_gutter,
            resource_height,
            styles,
            title_width,
            max_month_width,
            marked_date_offset,
            rect_corner_radius: 3.0,
            cols,
            rows,
            resources: chart_data.resources.clone(),
        })
    }

    fn render_chart(
        &self,
        add_resource_table: bool,
        rd: &RenderData,
    ) -> Result<String, Box<dyn Error>> {
        let width: f32 = rd.gutter.left
            + rd.title_width
            + rd.cols.iter().map(|col| col.width).sum::<f32>()
            + rd.gutter.right;
        let height = rd.gutter.top
            + (rd.rows.len() as f32 * rd.row_height)
            + (if add_resource_table {
                rd.resource_gutter.height() + rd.row_height
            } else {
                0.0
            })
            + rd.gutter.bottom;

        let style = build::elem("style").append(build::from_iter(rd.styles.iter()));

        let svg = build::elem("svg").with(attrs!(
            ("xmlns", "http://www.w3.org/2000/svg"),
            ("width", width),
            ("height", height),
            ("viewBox", format_move!("0 0 {} {}", width, height)),
            ("style", "background-color: white;")
        ));

        // Render all the chart rows
        let rows = build::elem("g").append(build::from_iter((0..=rd.rows.len()).map(|i| {
            build::from_closure(move |w| {
                let y = rd.gutter.top + (i as f32 * rd.row_height);
                let line;

                if i == 0 || i == rd.rows.len() {
                    line = build::single("line").with(attrs!(
                        ("class", "outer-lines"),
                        ("x1", rd.gutter.left),
                        ("y1", y),
                        ("x2", width - rd.gutter.right),
                        ("y2", y)
                    ));
                } else {
                    line = build::single("line").with(attrs!(
                        ("class", "inner-lines"),
                        ("x1", rd.gutter.left),
                        ("y1", y),
                        ("x2", width - rd.gutter.right),
                        ("y2", y)
                    ));
                }

                // Are we on one of the task rows?
                if i < rd.rows.len() {
                    let row: &RowRenderData = &rd.rows[i];
                    let text = build::elem("text")
                        .with(attrs!(
                            ("class", "item"),
                            ("x", rd.gutter.left + rd.row_gutter.left),
                            ("y", y + rd.row_gutter.top + rd.row_height / 2.0)
                        ))
                        .append(format_move!("{}", &row.title));

                    // Is this a task or a milestone?
                    if let Some(length) = row.length {
                        let bar = build::single("rect").with(attrs!(
                            (
                                "class",
                                format_move!(
                                    "resource-{}{}",
                                    row.resource_index,
                                    if row.open { "-open" } else { "-closed" }
                                )
                            ),
                            ("x", row.offset),
                            ("y", y + rd.row_gutter.top,),
                            ("rx", rd.rect_corner_radius),
                            ("ry", rd.rect_corner_radius),
                            ("width", length),
                            ("height", rd.row_height - rd.row_gutter.height())
                        ));

                        w.render(line.append(text).append(bar))
                    } else {
                        let n = (rd.row_height - rd.row_gutter.height()) / 2.0;

                        let milestone = build::single("path").with(attrs!(
                            ("class", "milestone"),
                            build::path([
                                M(row.offset - n, y + rd.row_gutter.top + n),
                                L_(n, -n),
                                L_(n, n),
                                L_(-n, n),
                                L_(-n, -n)
                            ])
                        ));

                        w.render(line.append(text).append(milestone))
                    }
                } else {
                    w.render(line)
                }
            })
        })));

        // Render all the charts columns
        let columns = build::elem("g").append(build::from_iter((0..=rd.cols.len()).map(|i| {
            build::from_closure(move |w| {
                let x: f32 = rd.gutter.left
                    + rd.title_width
                    + rd.cols.iter().take(i).map(|col| col.width).sum::<f32>();
                let line = build::single("line").with(attrs!(
                    ("class", "inner-lines"),
                    ("x1", x),
                    ("y1", rd.gutter.top),
                    ("x2", x),
                    (
                        "y2",
                        rd.gutter.top + ((rd.rows.len() as f32) * rd.row_height)
                    )
                ));

                if i < rd.cols.len() {
                    let text = build::elem("text")
                        .with(attrs!(
                            ("class", "heading"),
                            ("x", x + rd.max_month_width / 2.0),
                            (
                                "y",
                                // TODO(john): Use a more appropriate row height value here?
                                rd.gutter.top - rd.row_gutter.bottom - rd.row_height / 2.0
                            )
                        ))
                        .append(format_move!("{}", &rd.cols[i].month_name));

                    w.render(line.append(text))
                } else {
                    w.render(line)
                }
            })
        })));

        let tasks = build::elem("text")
            .with(attrs!(
                ("class", "heading task-heading"),
                ("x", rd.gutter.left + rd.row_gutter.left),
                // TODO(john): Use more appropriate row height value here?
                (
                    "y",
                    rd.gutter.top - rd.row_gutter.bottom - rd.row_height / 2.0
                )
            ))
            .append("Tasks");

        let title = build::elem("text")
            .with(attrs!(
                ("class", "title"),
                ("x", rd.gutter.left),
                // TODO(john): Use more appropriate row height value here?
                ("y", 25.0)
            ))
            .append(format_move!("{}", &rd.title));

        let marked = build::from_closure(move |w| {
            if let Some(offset) = rd.marked_date_offset {
                let marker = build::single("line").with(attrs!(
                    ("class", "marker"),
                    ("x1", offset),
                    ("y1", rd.gutter.top - 5.0),
                    ("x2", offset),
                    (
                        "y2",
                        rd.gutter.top + ((rd.rows.len() as f32) * rd.row_height) + 5.0
                    )
                ));

                w.render(marker)
            } else {
                w.render(build::single("g"))
            }
        });

        let resources =
            build::elem("g").append(build::from_iter((0..rd.resources.len()).map(|i| {
                build::from_closure(move |w| {
                    if add_resource_table {
                        let y = rd.gutter.top + ((rd.rows.len() as f32) * rd.row_height);
                        let block_width = rd.resource_height - rd.resource_gutter.height();
                        let text = build::elem("text")
                            .with(attrs!(
                                ("class", "resource"),
                                (
                                    "x",
                                    rd.resource_gutter.left + ((i + 1) as f32) * 100.0 - 5.0
                                ),
                                ("y", y + rd.resource_height / 2.0)
                            ))
                            .append(format_move!("{}", &rd.resources[i]));
                        let block = build::single("rect").with(attrs!(
                            ("class", format_move!("resource-{}-closed", i)),
                            (
                                "x",
                                rd.resource_gutter.left + ((i + 1) as f32) * 100.0 + 5.0
                            ),
                            ("y", y + rd.resource_gutter.top),
                            ("rx", rd.rect_corner_radius),
                            ("ry", rd.rect_corner_radius),
                            ("width", block_width),
                            ("height", block_width)
                        ));
                        w.render(block.append(text))
                    } else {
                        w.render(build::single("g"))
                    }
                })
            })));

        let all = svg
            .append(style)
            .append(title)
            .append(columns)
            .append(tasks)
            .append(rows)
            .append(marked)
            .append(resources);

        let mut output = String::new();
        hypermelon::render(all, &mut output)?;

        Ok(output)
    }
}
