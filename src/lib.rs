/// Generate a Gantt chart
use chrono::{Datelike, Duration, NaiveDate};
use clap::Parser;
use core::fmt::Arguments;
use serde::{Deserialize, Serialize};
use skia_safe::{
    paint, BlendMode, ClipOp, Color, EncodedImageFormat, Font, Paint, Path, Rect, Surface,
    TextBlob, Typeface,
};
use std::error::Error;
use std::fs;
use std::io::Write;
use std::path::PathBuf;

mod log_macros;

static MONTH_NAMES: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];
static MEASURED_TEXT: &str = "XgbQ";

#[derive(Parser)]
#[clap(version, about, long_about = None)]
struct Cli {
    /// Specify the JSON data file
    #[clap(value_name = "INPUT_FILE")]
    input_file: PathBuf,

    #[clap(long = "output", short, value_name = "OUTPUT_FILE")]
    output_file: PathBuf,
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
    #[allow(dead_code)]
    pub open: bool,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct ResourceData {
    #[allow(dead_code)]
    pub title: String,
    #[serde(rename = "color")]
    pub color_hex: u32,
}

#[derive(Deserialize, Serialize, Debug)]
pub struct ChartData {
    #[allow(dead_code)]
    pub title: String,
    pub resources: Vec<ResourceData>,
    pub items: Vec<ItemData>,
}

struct RenderData {
    width: f32,
    height: f32,
    line_thickness1: f32,
    line_color1: u32,
    line_thickness2: f32,
    line_color2: u32,
    chart_margin: f32,
    header_height: f32,
    item_font_size: f32,
    item_font_height: (f32, f32),
    item_padding: f32,
    item_max_width: f32,
    item_title_width: f32,
    colors: Vec<Color>,
    cols: Vec<ColumnRenderData>,
    rows: Vec<RowRenderData>,
}

struct RowRenderData {
    title: String,
    color_index: usize,
    start_offset: f32,
    /// End offset.  If not present then this is a milestone
    length: Option<f32>,
}

struct ColumnRenderData {
    width: f32,
    name_index: usize,
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

        let chart_data = Self::read_chart_file(&cli.input_file)?;
        let render_data = self.process_chart_data(&chart_data)?;
        let mut surface =
            Surface::new_raster_n32_premul((render_data.width as i32, render_data.height as i32))
                .ok_or("Unable to create Skia surface")?;

        self.draw_surface(&mut surface, &render_data)?;

        let image = surface.image_snapshot();
        let data = image
            .encode_to_data(EncodedImageFormat::PNG)
            .ok_or("Unable to encode data")?;

        let mut file = fs::File::create(cli.output_file)?;

        file.write_all(data.as_bytes())?;

        Ok(())
    }

    fn read_chart_file(chart_file: &PathBuf) -> Result<ChartData, Box<dyn Error>> {
        let content = fs::read_to_string(chart_file)?;
        let chart_data: ChartData = json5::from_str(&content)?;

        Ok(chart_data)
    }

    fn process_chart_data(
        self: &Self,
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

        fn measure_text_height(text: &str, size: f32) -> (f32, f32) {
            let paint = &mut Paint::default();

            paint.set_style(paint::Style::Fill).set_color(Color::BLACK);

            let font = Font::from_typeface(&Typeface::default(), size);
            let (_, rect) = font.measure_str(text, Some(&paint));

            (-rect.top, rect.bottom)
        }

        // TODO: Fail if only one task

        let mut rd = RenderData {
            width: 0.0,
            height: 0.0,
            header_height: 40.0, // TODO: Calculate this
            line_thickness1: 3.0,
            line_color1: 0xffaaaaaa,
            line_thickness2: 2.0,
            line_color2: 0xffdddddd,
            chart_margin: 10.0,
            item_padding: 6.0,
            item_font_size: 18.0,
            item_font_height: (0.0, 0.0),
            item_max_width: 70.0,
            item_title_width: 210.0,
            colors: chart_data
                .resources
                .iter()
                .map(|r| Color::new(r.color_hex))
                .collect(),
            cols: vec![],
            rows: vec![],
        };

        rd.height = rd.header_height + rd.chart_margin * 2.0;
        rd.width = rd.item_title_width + rd.chart_margin * 2.0;

        rd.item_font_height = measure_text_height(MEASURED_TEXT, rd.item_font_size);

        let bar_height = rd.item_font_height.0 + rd.item_font_height.1;
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
                // TODO: Be smarter about adding days and skip the weekends
                // TODO: Keep a shadow list of the _real_ duration including weekends
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

        date = start_date;

        while date <= end_date {
            let item_days = num_days_in_month(date.year(), date.month());
            let item_width = rd.item_max_width * (item_days as f32) / 31.0;

            num_item_days += item_days;
            all_items_width += item_width;

            rd.cols.push(ColumnRenderData {
                width: item_width,
                name_index: (date.month() - 1) as usize,
            });

            date = NaiveDate::from_ymd(date.year(), date.month() % 12 + 1, 1);
        }

        rd.width += all_items_width;
        date = start_date;

        let mut color_index: usize = 0;

        for item in chart_data.items.iter() {
            if let Some(item_start_date) = item.start_date {
                date = item_start_date;
            }

            let start_offset = rd.item_title_width
                + rd.chart_margin
                + ((date - start_date).num_days() as f32) / (num_item_days as f32)
                    * all_items_width;

            let mut length: Option<f32> = None;

            if let Some(item_days) = item.duration {
                // TODO: Use the shadow duration not the actual duration
                date += Duration::days(item_days);
                length = Some((item_days as f32) / (num_item_days as f32) * all_items_width);
            }

            if let Some(item_resource_index) = item.resource_index {
                color_index = item_resource_index;
            }

            rd.height += bar_height + rd.item_padding * 2.0;
            rd.rows.push(RowRenderData {
                title: item.title.clone(),
                color_index,
                start_offset,
                length,
            });
        }

        Ok(rd)
    }

    fn draw_surface(
        self: &Self,
        surface: &mut Surface,
        rd: &RenderData,
    ) -> Result<(), Box<dyn Error>> {
        const BAR_RADIUS: f32 = 3.0;

        let canvas = surface.canvas();

        canvas.draw_color(Color::WHITE, BlendMode::default());

        // Draw the horizontal lines
        let (line_paint1, line_paint2) = (&mut Paint::default(), &mut Paint::default());

        line_paint1
            .set_style(paint::Style::Stroke)
            .set_stroke_width(rd.line_thickness1)
            .set_color(Color::from(rd.line_color1));
        line_paint2
            .set_style(paint::Style::Stroke)
            .set_stroke_width(rd.line_thickness2)
            .set_color(Color::from(rd.line_color2));

        let task_paints = rd
            .colors
            .iter()
            .map(|color| {
                let mut paint: Paint = Paint::default();

                paint
                    .set_style(paint::Style::Fill)
                    .set_color(*color)
                    .set_anti_alias(true);

                return paint;
            })
            .collect::<Vec<_>>();

        let black_paint = &mut Paint::default();

        let font = Font::from_typeface(&Typeface::default(), rd.item_font_size);

        black_paint
            .set_style(paint::Style::Fill)
            .set_color(Color::BLACK);

        let mut line_begin = (
            rd.chart_margin + rd.item_title_width,
            rd.chart_margin + rd.header_height,
        );
        let mut line_end = (line_begin.0, rd.height - rd.chart_margin);

        for i in 0..=rd.cols.len() {
            canvas.draw_path(&Path::line(line_begin, line_end), line_paint2);

            if i < rd.cols.len() {
                let col = &rd.cols[i];
                let month_name = MONTH_NAMES[col.name_index];
                let (_, text_rect) = font.measure_str(month_name, Some(&black_paint));

                canvas.draw_text_blob(
                    &TextBlob::from_str(month_name, &font).unwrap(),
                    (
                        line_begin.0 + (col.width - text_rect.width()) / 2.0,
                        line_begin.1 - rd.item_padding,
                    ),
                    &black_paint,
                );

                line_begin.0 += col.width;
                line_end.0 += col.width;
            }
        }

        line_begin = (rd.chart_margin, rd.chart_margin + rd.header_height);
        line_end = (
            rd.width - rd.chart_margin,
            rd.chart_margin + rd.header_height,
        );

        let bar_height = rd.item_font_height.0 + rd.item_font_height.1;
        let line_height = bar_height + rd.item_padding * 2.0;

        for i in 0..=rd.rows.len() {
            if i == 0 || i == rd.rows.len() {
                canvas.draw_path(&Path::line(line_begin, line_end), line_paint1);
            } else {
                canvas.draw_path(&Path::line(line_begin, line_end), line_paint2);
            };

            if i < rd.rows.len() {
                let row: &RowRenderData = &rd.rows[i];

                canvas.save();
                canvas.clip_rect(
                    &Rect::new(
                        line_begin.0,
                        line_begin.1 + rd.item_padding,
                        line_begin.0 + rd.item_title_width - rd.item_padding,
                        line_begin.1 + line_height - rd.item_padding,
                    ),
                    ClipOp::Intersect,
                    Some(true),
                );
                canvas.draw_text_blob(
                    &TextBlob::from_str(&row.title, &font).unwrap(),
                    (
                        line_begin.0,
                        line_begin.1 + rd.item_font_height.0 + rd.item_padding,
                    ),
                    &black_paint,
                );
                canvas.restore();

                if let Some(length) = row.length {
                    canvas.draw_round_rect(
                        Rect::from_point_and_size(
                            (row.start_offset, line_begin.1 + rd.item_padding),
                            (length, bar_height),
                        ),
                        BAR_RADIUS,
                        BAR_RADIUS,
                        &task_paints[row.color_index],
                    );
                } else {
                    let mut path = Path::new();

                    path.move_to((row.start_offset, line_begin.1 + rd.item_padding));
                    path.line_to((
                        row.start_offset + bar_height / 2.0,
                        line_begin.1 + bar_height / 2.0 + rd.item_padding,
                    ));
                    path.line_to((
                        row.start_offset,
                        line_begin.1 + bar_height + rd.item_padding,
                    ));
                    path.line_to((
                        row.start_offset - bar_height / 2.0,
                        line_begin.1 + bar_height / 2.0 + rd.item_padding,
                    ));
                    path.close();

                    canvas.draw_path(&path, black_paint);
                }
            }

            line_begin.1 += line_height;
            line_end.1 += line_height;
        }

        Ok(())
    }
}
