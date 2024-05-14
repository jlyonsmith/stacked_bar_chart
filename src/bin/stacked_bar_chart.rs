use core::fmt::Arguments;
use stacked_bar_chart::{error, StackedBarChartLog, StackedBarChartTool};
use yansi::Paint;

struct StackedBarChartLogger;

impl StackedBarChartLogger {
    fn new() -> StackedBarChartLogger {
        StackedBarChartLogger {}
    }
}

impl StackedBarChartLog for StackedBarChartLogger {
    fn output(self: &Self, args: Arguments) {
        println!("{}", args);
    }
    fn warning(self: &Self, args: Arguments) {
        eprintln!("{}", format!("warning: {}", Paint::yellow(args)));
    }
    fn error(self: &Self, args: Arguments) {
        eprintln!("{}", format!("error: {}", Paint::red(args)));
    }
}

fn main() {
    let logger = StackedBarChartLogger::new();

    if let Err(error) = StackedBarChartTool::new(&logger).run(std::env::args_os()) {
        error!(logger, "{}", error);
        std::process::exit(1);
    }
}
