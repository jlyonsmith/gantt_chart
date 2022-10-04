use colored::Colorize;
use core::fmt::Arguments;
use gantt_chart::{error, GanttChartLog, GanttChartTool};

struct GanttChartLogger;

impl GanttChartLogger {
    fn new() -> GanttChartLogger {
        GanttChartLogger {}
    }
}

impl GanttChartLog for GanttChartLogger {
    fn output(self: &Self, args: Arguments) {
        println!("{}", args);
    }
    fn warning(self: &Self, args: Arguments) {
        eprintln!("{}", format!("warning: {}", args).yellow());
    }
    fn error(self: &Self, args: Arguments) {
        eprintln!("{}", format!("error: {}", args).red());
    }
}

fn main() {
    let logger = GanttChartLogger::new();

    if let Err(error) = GanttChartTool::new(&logger).run(std::env::args_os()) {
        error!(logger, "{}", error);
        std::process::exit(1);
    }
}
