use openpilot_opencv_runtime as cv;
use serde::Deserialize;
use serde_json::json;
use std::io::{self, Read};
#[derive(Deserialize)]
struct Input {
    width: u32,
    height: u32,
    rgb: Vec<u8>,
    mask: Vec<u8>,
    template: Vec<u8>,
    template_width: u32,
    template_height: u32,
    template_mask: Vec<u8>,
}
fn image(
    data: &[u8],
    size: cv::Dimensions,
    format: cv::Format,
) -> Result<cv::ImageView<'_>, cv::Error> {
    cv::ImageView::new(data, cv::ImageLayout::new(size, format)?)
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    cv::initialize(1)?;
    let mut text = String::new();
    io::stdin().read_to_string(&mut text)?;
    let cases: Vec<Input> = serde_json::from_str(&text)?;
    let mut output = Vec::new();
    for i in cases {
        let dims = cv::Dimensions::new(i.width, i.height)?;
        let rgb = image(&i.rgb, dims, cv::Format::Rgb)?;
        let mask = image(&i.mask, dims, cv::Format::Gray)?;
        let gray = cv::rgb_to_gray(rgb)?;
        let hsv = cv::rgb_to_hsv(rgb)?;
        let components = cv::connected_components(mask)?;
        let opened = cv::morphology_open(mask, cv::Dimensions::new(3, 2)?)?;
        let contours = cv::external_contours(opened.view())?;
        let patch = cv::Dimensions::new(i.template_width, i.template_height)?;
        let template = image(&i.template, patch, cv::Format::Gray)?;
        let filter = if i.template_mask.is_empty() {
            None
        } else {
            Some(image(&i.template_mask, patch, cv::Format::Gray)?)
        };
        let tophat = cv::morphology_tophat(mask, cv::Dimensions::new(15, 15)?)?;
        let distance = cv::distance_transform(mask)?;
        let scores = cv::match_template(gray.view(), template, filter)?;
        output.push(json!({"gray":gray.view().data(),"hsv":hsv.view().data(),"labels":components.labels,
   "stats":components.stats.iter().map(|c|json!({"bounds":c.bounds,"area":c.area,"center":c.center})).collect::<Vec<_>>(),
   "opened":opened.view().data(),"contours":contours.iter().map(|c|json!({"bounds":[c.bounds.x,c.bounds.y,c.bounds.width as i32,c.bounds.height as i32],"area":c.area})).collect::<Vec<_>>(),"scores":scores,"tophat":tophat.view().data(),"distance":distance}));
    }
    println!("{}", serde_json::to_string(&output)?);
    Ok(())
}
