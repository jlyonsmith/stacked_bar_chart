mod log_macros;

use clap::Parser;
use core::fmt::Arguments;
use easy_error::{self, bail, ResultExt};
use rand::prelude::*;
use serde::Deserialize;
use std::{
    error::Error,
    fs::File,
    io::{self, Read, Write},
    path::PathBuf,
};
use svg::{
    node::{element::path, *},
    Document,
};

const GOLDEN_RATIO_CONJUGATE: f32 = 0.618033988749895;

pub trait StackedBarChartLog {
    fn output(self: &Self, args: Arguments);
    fn warning(self: &Self, args: Arguments);
    fn error(self: &Self, args: Arguments);
}

pub struct StackedBarChartTool<'a> {
    log: &'a dyn StackedBarChartLog,
}

#[derive(Parser)]
#[clap(version, about, long_about = None)]
struct Cli {
    /// Disable colors in output
    #[arg(long = "no-color", short = 'n', env = "NO_CLI_COLOR")]
    no_color: bool,

    /// The input file
    #[arg(value_name = "INPUT_FILE")]
    input_file: Option<PathBuf>,

    /// The output file
    #[arg(value_name = "OUTPUT_FILE")]
    output_file: Option<PathBuf>,
}

impl Cli {
    fn get_output(&self) -> Result<Box<dyn Write>, Box<dyn Error>> {
        match self.output_file {
            Some(ref path) => File::create(path)
                .context(format!(
                    "Unable to create file '{}'",
                    path.to_string_lossy()
                ))
                .map(|f| Box::new(f) as Box<dyn Write>)
                .map_err(|e| Box::new(e) as Box<dyn Error>),
            None => Ok(Box::new(io::stdout())),
        }
    }

    fn get_input(&self) -> Result<Box<dyn Read>, Box<dyn Error>> {
        match self.input_file {
            Some(ref path) => File::open(path)
                .context(format!("Unable to open file '{}'", path.to_string_lossy()))
                .map(|f| Box::new(f) as Box<dyn Read>)
                .map_err(|e| Box::new(e) as Box<dyn Error>),
            None => Ok(Box::new(io::stdin())),
        }
    }
}

#[derive(Deserialize, Debug, Clone)]
pub struct ChartData {
    pub title: String,
    pub units: String,
    pub categories: Vec<String>,
    pub items: Vec<ItemData>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct ItemData {
    pub key: String,
    pub values: Vec<f64>,
}

#[derive(Debug)]
struct Gutter {
    left: f64,
    top: f64,
    right: f64,
    bottom: f64,
}

impl Gutter {
    pub fn top_bottom(&self) -> f64 {
        self.bottom + self.top
    }

    pub fn left_right(&self) -> f64 {
        self.right + self.left
    }
}

#[derive(Debug)]
struct BarData {
    label: String,
    values: Vec<f64>,
}

#[derive(Debug)]
struct RenderData {
    title: String,
    categories: Vec<String>,
    gutter: Gutter,
    y_axis_height: f64,
    y_axis_range: (f64, f64),
    y_axis_interval: f64,
    y_axis_decimal_places: usize,
    x_axis_item_width: f64,
    bar_data: Vec<BarData>,
    styles: Vec<String>,
    legend_gutter: Gutter,
    legend_rect_size: f64,
    legend_rect_corner_radius: f64,
}

impl<'a> StackedBarChartTool<'a> {
    pub fn new(log: &'a dyn StackedBarChartLog) -> StackedBarChartTool {
        StackedBarChartTool { log }
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

        let chart_data = Self::read_chart_file(cli.get_input()?)?;
        let render_data = self.process_chart_data(&chart_data)?;
        let document = self.render_chart(&render_data)?;

        Self::write_svg_file(cli.get_output()?, &document)?;

        Ok(())
    }

    fn read_chart_file(mut reader: Box<dyn Read>) -> Result<ChartData, Box<dyn Error>> {
        let mut content = String::new();

        reader.read_to_string(&mut content)?;

        let chart_data: ChartData = json5::from_str(&content)?;

        Ok(chart_data)
    }

    fn write_svg_file(writer: Box<dyn Write>, document: &Document) -> Result<(), Box<dyn Error>> {
        svg::write(writer, document)?;

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

    fn process_chart_data(self: &Self, cd: &ChartData) -> Result<RenderData, Box<dyn Error>> {
        // Generate random resource colors based on https://martin.ankerl.com/2009/12/09/how-to-create-random-colors-programmatically/
        let mut rng = rand::thread_rng();
        let mut h: f32 = rng.gen();

        let mut styles = vec![
            ".labels{fill:rgb(0,0,0);font-size:10;font-family:Arial}".to_string(),
            ".title{font-family:Arial;font-size:12;text-anchor:middle;}".to_string(),
            ".legend{font-family:Arial;font-size:12pt;text-anchor:left;}".to_string(),
            ".axis{fill:none;stroke:rgb(0,0,0);stroke-width:1;}".to_owned(),
            ".y-labels{text-anchor:end;}".to_owned(),
        ];

        let mut bar_data = vec![];
        let mut y_axis_range: (f64, f64) = (0.0, f64::MIN);

        for tuple in cd.items.iter().enumerate() {
            let (index, item) = tuple;

            if item.values.len() < cd.categories.len() {
                bail!(
                    "Item {} needs {} values and has {}",
                    index,
                    cd.categories.len(),
                    item.values.len()
                );
            }

            let sum = item.values.iter().sum();

            if sum > y_axis_range.1 {
                y_axis_range.1 = sum;
            }

            let rgb = Self::hsv_to_rgb(h, 0.5, 0.5);

            styles.push(format!(
                ".category-{}{{fill:#{1:06x};stroke-width:0}}",
                index, rgb,
            ));

            bar_data.push(BarData {
                label: item.key.to_string(),
                values: item.values.clone(),
            });

            h = (h + GOLDEN_RATIO_CONJUGATE) % 1.0;
        }

        let y_axis_max_intervals = 20.0;
        let y_axis_interval = (10.0_f64).powf(((y_axis_range.1 - y_axis_range.0).log10()).ceil())
            / y_axis_max_intervals;
        let decimal_places = y_axis_interval.log10();
        let y_axis_decimal_places = if decimal_places < 0.0 {
            decimal_places.abs().ceil() as usize
        } else {
            0
        };

        y_axis_range = (
            f64::floor(y_axis_range.0 / y_axis_interval) * y_axis_interval,
            f64::ceil(y_axis_range.1 / y_axis_interval) * y_axis_interval,
        );

        let gutter = Gutter {
            top: 40.0,
            bottom: 40.0,
            left: 40.0,
            right: 40.0,
        };
        let x_axis_item_width = 30.0;
        let legend_rect_size = 20.0;
        let legend_gutter = Gutter {
            top: 10.0,
            bottom: 80.0,
            left: 40.0,
            right: 10.0,
        };

        Ok(RenderData {
            title: cd.title.to_string(),
            categories: cd.categories.clone(),
            gutter,
            x_axis_item_width,
            y_axis_height: 300.0,
            y_axis_interval,
            y_axis_range,
            y_axis_decimal_places,
            bar_data,
            legend_gutter,
            legend_rect_size,
            legend_rect_corner_radius: 3.0,
            styles,
        })
    }

    fn render_chart(self: &Self, rd: &RenderData) -> Result<Document, Box<dyn Error>> {
        let width =
            rd.gutter.left + ((rd.bar_data.len() as f64) * rd.x_axis_item_width) + rd.gutter.right;
        let height = rd.gutter.top_bottom()
            + rd.y_axis_height
            + rd.legend_gutter.top_bottom()
            + rd.legend_rect_size;
        let num_y_labels =
            ((rd.y_axis_range.1 - rd.y_axis_range.0) / rd.y_axis_interval) as usize + 1;
        let scale =
            |n: &f64| -> f64 { n * (rd.y_axis_height / (rd.y_axis_range.1 - rd.y_axis_range.0)) };
        let mut document = Document::new()
            .set("xmlns", "http://www.w3.org/2000/svg")
            .set("width", width)
            .set("height", height)
            .set("viewBox", format!("0 0 {} {}", width, height))
            .set("style", "background-color: white;");
        let style = element::Style::new(rd.styles.join("\n"));
        let axis = element::Polyline::new().set("class", "axis").set(
            "points",
            vec![
                (rd.gutter.left, rd.gutter.top),
                (rd.gutter.left, rd.gutter.top + rd.y_axis_height),
                (width - rd.gutter.right, rd.gutter.top + rd.y_axis_height),
            ],
        );
        let mut x_axis_labels = element::Group::new().set("class", "labels");

        for i in 0..rd.bar_data.len() {
            x_axis_labels.append(element::Text::new(format!("{}", rd.bar_data[i].label)).set(
                "transform",
                format!(
                    "translate({},{}) rotate(45)",
                    rd.gutter.left + (i as f64 * rd.x_axis_item_width) + rd.x_axis_item_width / 2.0,
                    rd.gutter.top + rd.y_axis_height + 15.0
                ),
            ));
        }

        let mut y_axis_labels = element::Group::new().set("class", "labels y-labels");

        for i in 0..num_y_labels {
            let n = i as f64 * rd.y_axis_interval;

            y_axis_labels.append(
                element::Text::new(format!(
                    "{0:.1$}",
                    n + rd.y_axis_range.0,
                    rd.y_axis_decimal_places
                ))
                .set(
                    "transform",
                    format!(
                        "translate({},{})",
                        rd.gutter.left - 10.0,
                        rd.gutter.top + rd.y_axis_height - f64::floor(scale(&n)) + 5.0
                    ),
                ),
            );
        }

        let mut bars = element::Group::new();
        let bar_width = rd.x_axis_item_width / 2.0;

        for i in 0..rd.bar_data.len() {
            let bar_datum = &rd.bar_data[i];
            let heights = bar_datum.values.iter().map(scale).collect::<Vec<f64>>();
            let mut bar = element::Group::new();
            let mut y = rd.gutter.top + rd.y_axis_height;

            for j in 0..heights.len() {
                bar.append(
                    element::Path::new()
                        .set("class", format!("category-{}", j))
                        .set(
                            "d",
                            path::Data::new()
                                .move_to((
                                    rd.gutter.left
                                        + (i as f64 * rd.x_axis_item_width)
                                        + bar_width / 2.0,
                                    y,
                                ))
                                .line_by((bar_width, 0.0))
                                .line_by((0.0, -heights[j]))
                                .line_by((-bar_width, 0.0))
                                .close(),
                        ),
                );

                y -= heights[j];
            }

            bars.append(bar);
        }

        let mut legend = element::Group::new();
        let text_width = (width - rd.legend_gutter.left_right()) / (rd.bar_data.len() as f64);

        for i in 0..rd.categories.len() {
            let y = rd.gutter.top_bottom() + rd.y_axis_height + rd.legend_gutter.top;
            let block = element::Rectangle::new()
                .set("class", format!("category-{}", i))
                .set("x", rd.legend_gutter.left + (i as f64) * text_width)
                .set("y", y)
                .set("rx", rd.legend_rect_corner_radius)
                .set("ry", rd.legend_rect_corner_radius)
                .set("width", rd.legend_rect_size)
                .set("height", rd.legend_rect_size);

            legend.append(block);

            let text = element::Text::new(format!("{}", &rd.categories[i]))
                .set("class", "legend")
                .set(
                    "transform",
                    format!(
                        "translate({},{}) rotate(45)",
                        rd.legend_gutter.left + (i as f64) * text_width,
                        y + rd.legend_rect_size * 1.5
                    ),
                );

            legend.append(text);
        }

        let title = element::Text::new(format!("{}", &rd.title))
            .set("class", "title")
            .set("x", width / 2.0)
            .set("y", rd.gutter.top / 2.0);

        document.append(style);
        document.append(bars);
        document.append(axis);
        document.append(x_axis_labels);
        document.append(y_axis_labels);
        document.append(title);
        document.append(legend);

        Ok(document)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_test() {
        struct TestLogger;

        impl TestLogger {
            fn new() -> TestLogger {
                TestLogger {}
            }
        }

        impl StackedBarChartLog for TestLogger {
            fn output(self: &Self, _args: Arguments) {}
            fn warning(self: &Self, _args: Arguments) {}
            fn error(self: &Self, _args: Arguments) {}
        }

        let logger = TestLogger::new();
        let mut tool = StackedBarChartTool::new(&logger);
        let args: Vec<std::ffi::OsString> = vec!["".into(), "--help".into()];

        tool.run(args).unwrap();
    }
}
