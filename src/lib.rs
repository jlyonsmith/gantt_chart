/// Generate a Gantt chart
use chrono::{Duration, NaiveDate};
use clap::Parser;
use core::fmt::Arguments;
use serde::Deserialize;
use skia_safe::{
    paint, BlendMode, Color, EncodedImageFormat, Font, Paint, Path, RRect, Rect, Surface, TextBlob,
    TextEncoding, Typeface,
};
use std::error::Error;
use std::fs;
use std::io::Write;
use std::ops::Add;
use std::path::PathBuf;

mod log_macros;

pub(crate) mod resources {
    use skia_safe::{Data, Image};

    pub fn color_wheel() -> Image {
        let bytes = include_bytes!("resources/color_wheel.png");
        let data = Data::new_copy(bytes);
        Image::from_encoded(data).unwrap()
    }
}

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

#[derive(Deserialize, Debug)]
pub struct ItemData {
    title: String,
    duration: Option<i64>,
    #[serde(rename = "startDate")]
    start_date: Option<NaiveDate>,
    #[serde(rename = "resource")]
    resource_index: Option<usize>,
}

#[derive(Deserialize, Debug)]
pub struct ResourceData {
    title: String,
    #[serde(rename = "color")]
    color_hex: String,
}

#[derive(Deserialize, Debug)]
pub struct ChartData {
    title: String,
    resources: Vec<ResourceData>,
    items: Vec<ItemData>,
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
    item_height: f32,
    item_width: f32,
    item_title_width: f32,
    month_names: Vec<String>,
    colors: Vec<u32>,
    rows: Vec<RowRenderData>,
}

struct RowRenderData {
    color_index: usize,
    /// End offset.  If not present then this is a milestone
    end_offset: Option<f32>,
    start_offset: f32,
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
            Ok(m) => m,
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
        // TODO: Fail if only one task

        let mut rd = RenderData {
            width: 0.0,
            height: 0.0,
            header_height: 40.0,
            line_thickness1: 3.0,
            line_color1: 0xffaaaaaa,
            line_thickness2: 2.0,
            line_color2: 0xffdddddd,
            chart_margin: 10.0,
            item_height: 30.0,
            item_width: 70.0,
            item_title_width: 210.0,
            month_names: vec![],
            colors: vec![],
            rows: vec![],
        };
        let mut start_date = NaiveDate::MAX;
        let mut end_date = NaiveDate::MIN;
        let mut date = NaiveDate::MIN;
        let mut color_index: usize = 0;

        // TODO: For each item we need a start & end date which excludes weekends

        for (i, item) in chart_data.items.iter().enumerate() {
            if let Some(item_start_date) = item.start_date {
                date = item_start_date;

                if item_start_date < start_date {
                    // TODO: Ensure the date is not on a weekend
                    start_date = date;
                }
            } else if i == 0 {
                return Err(From::from(format!("First item must contain a start date")));
            }

            if let Some(item_days) = item.duration {
                // TODO: Be smarter about adding days and skip weekends
                date += Duration::days(item_days);
            }

            if end_date < date {
                end_date = date;
            }

            if let Some(item_resource_index) = item.resource_index {
                color_index = item_resource_index;
            } else if i == 0 {
                return Err(From::from(format!("First item must contain a start date")));
            }

            rd.rows.push(RowRenderData {
                color_index,
                start_offset: 0.0,
                end_offset: None,
            });
        }

        let date = start_date;

        // TODO: Iterate from start to end dates getting month names
        // TODO: Grab the number of days in each month
        for _ in 1..=12 {
            rd.month_names.push(date.format("%b").to_string());
        }

        // TODO: Naively go through each task and generate the start and end offset (ignoring weekends)

        // TODO: Generate the array of colors

        rd.height = rd.header_height + rd.chart_margin * 2.0;
        rd.width = rd.item_title_width
            + (rd.month_names.len() as f32) * rd.item_width
            + rd.chart_margin * 2.0;

        for _ in chart_data.items.iter() {
            rd.rows.push(RowRenderData {
                color_index: 0,
                end_offset: Some(0.0),
                start_offset: 0.0,
            });
            rd.height += rd.item_height;
        }

        Ok(rd)
    }

    fn draw_surface(
        self: &Self,
        surface: &mut Surface,
        rd: &RenderData,
    ) -> Result<(), Box<dyn Error>> {
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

        let mut line_begin = (
            rd.chart_margin + rd.item_title_width,
            rd.chart_margin + rd.header_height,
        );
        let mut line_end = (line_begin.0, rd.height - rd.chart_margin);

        for _ in 0..=rd.month_names.len() {
            canvas.draw_path(&Path::line(line_begin, line_end), line_paint2);

            line_begin.0 += rd.item_width;
            line_end.0 += rd.item_width;
        }

        line_begin = (rd.chart_margin, rd.chart_margin + rd.header_height);
        line_end = (
            rd.width - rd.chart_margin,
            rd.chart_margin + rd.header_height,
        );

        for i in 0..=rd.rows.len() {
            if i == 0 || i == rd.rows.len() {
                canvas.draw_path(&Path::line(line_begin, line_end), line_paint1);
            } else {
                canvas.draw_path(&Path::line(line_begin, line_end), line_paint2);
            };

            line_begin.1 += rd.item_height;
            line_end.1 += rd.item_height;
        }

        // let paint2 = Paint::default();
        // let text = "Hello, Skia!";
        // let font = Font::from_typeface(&Typeface::default(), 18.0);
        // let text_blob = TextBlob::from_str(text, &font).unwrap();
        // let (text_scalar, text_rect) =
        //     font.measure_text(text.as_bytes(), TextEncoding::UTF8, Some(&paint2));
        // output!(
        //     self.log,
        //     "{}, {}, {}",
        //     text_scalar,
        //     text_rect.width(),
        //     text_rect.height()
        // );
        // canvas.draw_text_blob(&text_blob, (50, 25), &paint2);

        Ok(())
    }
}
