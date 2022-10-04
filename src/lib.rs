/// Generate a Gantt chart
use chrono::NaiveDate;
use clap::Parser;
use core::fmt::Arguments;
use serde::Deserialize;
use std::error::Error;
use std::fs;
use std::io::Write;
use std::path::PathBuf;

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

#[macro_export]
macro_rules! output {
  ($log: expr, $fmt: expr) => {
    $log.output(format_args!($fmt))
  };
  ($log: expr, $fmt: expr, $($args: tt)+) => {
    $log.output(format_args!($fmt, $($args)+))
  };
}
#[macro_export]
macro_rules! warning {
  ($log: expr, $fmt: expr) => {
    $log.warning(format_args!($fmt))
  };
  ($log: expr, $fmt: expr, $($args: tt)+) => {
    $log.warning(format_args!($fmt, $($args)+))
  };
}

#[macro_export]
macro_rules! error {
  ($log: expr, $fmt: expr) => {
    $log.error(format_args!($fmt))
  };
  ($log: expr, $fmt: expr, $($args: tt)+) => {
    $log.error(format_args!($fmt, $($args)+))
  };
}

pub struct GanttChartTool<'a> {
    log: &'a dyn GanttChartLog,
}

#[derive(Deserialize, Debug)]
pub struct TaskData {
    name: String,
    days: u16,
}

#[derive(Deserialize, Debug)]
pub struct MilestoneData {
    name: String,
    date: NaiveDate,
}

#[derive(Deserialize, Debug)]
pub struct ResourceData {
    name: String,
    #[serde(rename = "color")]
    colorHex: String,
    tasks: Vec<TaskData>,
}

#[derive(Deserialize, Debug)]
pub struct ChartData {
    name: String,
    #[serde(rename = "startDate")]
    start_date: NaiveDate,
    resources: Vec<ResourceData>,
    milestones: Vec<MilestoneData>,
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

        self.create_png(&cli.output_file)?;

        Ok(())
    }

    fn read_chart_file(chart_file: &PathBuf) -> Result<ChartData, Box<dyn Error>> {
        let content = fs::read_to_string(chart_file)?;
        let chart_data: ChartData = json5::from_str(&content)?;

        Ok(chart_data)
    }

    fn create_png(self: &Self, png_file: &PathBuf) -> Result<(), Box<dyn Error>> {
        use skia_safe::{
            paint, BlendMode, Color, EncodedImageFormat, Font, Paint, Path, RRect, Rect, Surface,
            TextBlob, TextEncoding, Typeface,
        };

        let (width, height) = (512, 512);
        let mut surface = Surface::new_raster_n32_premul((width, height))
            .ok_or("Unable to create Skia surface")?;
        let canvas = surface.canvas();

        let image = resources::color_wheel();

        canvas.draw_color(Color::WHITE, BlendMode::default());

        let mut paint = Paint::default();

        paint
            .set_style(paint::Style::Stroke)
            .set_stroke_width(4.0)
            .set_color(Color::RED);

        let mut rect = Rect::from_point_and_size((50.0, 50.0), (40.0, 60.0));
        canvas.draw_rect(rect, &paint);

        let oval = RRect::new_oval(rect).with_offset((40.0, 60.0));
        paint.set_color(Color::BLUE);
        canvas.draw_rrect(oval, &paint);

        paint.set_color(Color::CYAN);
        canvas.draw_circle((180.0, 50.0), 25.0, &paint);

        rect = rect.with_offset((80.0, 0.0));
        paint.set_color(Color::YELLOW);
        canvas.draw_round_rect(rect, 10.0, 10.0, &paint);

        let mut path = Path::default();
        path.cubic_to((768.0, 0.0), (-512.0, 256.0), (256.0, 256.0));
        paint.set_color(Color::GREEN);
        canvas.draw_path(&path, &paint);

        canvas.draw_image(&image, (128.0, 128.0), Some(&paint));

        let rect2 = Rect::from_point_and_size((0.0, 0.0), (40.0, 60.0));
        canvas.draw_image_rect(&image, None, rect2, &paint);

        let paint2 = Paint::default();

        let text = "Hello, Skia!";
        let font = Font::from_typeface(&Typeface::default(), 18.0);
        let text_blob = TextBlob::from_str(text, &font).unwrap();

        let (text_scalar, text_rect) =
            font.measure_text(text.as_bytes(), TextEncoding::UTF8, Some(&paint2));

        output!(
            self.log,
            "{}, {}, {}",
            text_scalar,
            text_rect.width(),
            text_rect.height()
        );

        canvas.draw_text_blob(&text_blob, (50, 25), &paint2);

        let image = surface.image_snapshot();
        let data = image
            .encode_to_data(EncodedImageFormat::PNG)
            .ok_or("Unable to encode data")?;

        let mut file = fs::File::create(png_file)?;

        file.write_all(data.as_bytes())?;

        Ok(())
    }
}
