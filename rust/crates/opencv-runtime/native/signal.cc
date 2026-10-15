#include "bridge.h"
#include "check.h"
#include <opencv2/imgproc.hpp>
namespace openpilot_opencv {
rust::Vec<std::uint8_t> rgb_gray(rust::Slice<const std::uint8_t> data, Dimensions size) {
  const auto input = borrowed_image(data, Layout{size, 3});
  cv::Mat result; cv::cvtColor(input, result, cv::COLOR_RGB2GRAY); return owned_bytes(result);
}
rust::Vec<std::uint8_t> rgb_hsv(rust::Slice<const std::uint8_t> data, Dimensions size) {
  const auto input = borrowed_image(data, Layout{size, 3});
  cv::Mat result; cv::cvtColor(input, result, cv::COLOR_RGB2HSV); return owned_bytes(result);
}
Components components(rust::Slice<const std::uint8_t> data, Dimensions size) {
  const auto input = borrowed_image(data, Layout{size, 1});
  cv::Mat labels, stats, centers;
  const int count = cv::connectedComponentsWithStats(input, labels, stats, centers, 8, CV_32S);
  Components result;
  if (labels.type() != CV_32SC1) throw std::runtime_error("OpenCV returned non-integer labels");
  const cv::Mat contiguous = labels.isContinuous() ? labels : labels.clone();
  result.labels = copy_labels(rust::Slice<const std::int32_t>(contiguous.ptr<std::int32_t>(), contiguous.total()));
  for (int i=0; i<count; ++i) result.stats.push_back(Component{stats.at<int>(i,0),stats.at<int>(i,1),stats.at<int>(i,2),stats.at<int>(i,3),stats.at<int>(i,4),centers.at<double>(i,0),centers.at<double>(i,1)});
  return result;
}
rust::Vec<std::uint8_t> morphology_open(rust::Slice<const std::uint8_t> data, Dimensions size, Dimensions kernel) {
  const auto input = borrowed_image(data, Layout{size, 1});
  checked_size(kernel,1);
  const auto element=cv::Mat::ones(static_cast<int>(kernel.height),static_cast<int>(kernel.width),CV_8UC1);
  cv::Mat result; cv::morphologyEx(input,result,cv::MORPH_OPEN,element); return owned_bytes(result);
}
rust::Vec<std::uint8_t> morphology_tophat(rust::Slice<const std::uint8_t> data, Dimensions size, Dimensions kernel) {
  const auto input=borrowed_image(data,Layout{size,1}); checked_size(kernel,1);
  const auto element=cv::Mat::ones(static_cast<int>(kernel.height),static_cast<int>(kernel.width),CV_8UC1);
  cv::Mat result; cv::morphologyEx(input,result,cv::MORPH_TOPHAT,element); return owned_bytes(result);
}
rust::Vec<float> distance_transform(rust::Slice<const std::uint8_t> data, Dimensions size) {
  const auto input=borrowed_image(data,Layout{size,1});
  cv::Mat result;cv::distanceTransform(input,result,cv::DIST_L2,5);
  return owned_scores(result);
}
rust::Vec<Contour> external_contours(rust::Slice<const std::uint8_t> data, Dimensions size) {
  const auto input = borrowed_image(data, Layout{size, 1});
  std::vector<std::vector<cv::Point>> contours;
  cv::findContours(input,contours,cv::RETR_EXTERNAL,cv::CHAIN_APPROX_SIMPLE);
  rust::Vec<Contour> output;
  for (const auto& contour:contours) {
    const auto b=cv::boundingRect(contour);
    output.push_back(Contour{Rect{b.x,b.y,static_cast<std::uint32_t>(b.width),static_cast<std::uint32_t>(b.height)},cv::contourArea(contour)});
  }
  return output;
}
static rust::Vec<float> match_template_mode(rust::Slice<const std::uint8_t> data, Dimensions size, rust::Slice<const std::uint8_t> patch, Dimensions patch_size, rust::Slice<const std::uint8_t> mask, int method) {
  const auto input=borrowed_image(data,Layout{size,1});
  const auto pattern=borrowed_image(patch,Layout{patch_size,1});
  if (patch_size.width>size.width || patch_size.height>size.height) throw std::invalid_argument("template exceeds source");
  cv::Mat filter;
  if (!mask.empty()) filter=borrowed_image(mask,Layout{patch_size,1});
  cv::Mat scores; cv::matchTemplate(input,pattern,scores,method,filter);
  return owned_scores(scores);
}
rust::Vec<float> match_template(rust::Slice<const std::uint8_t> data, Dimensions size, rust::Slice<const std::uint8_t> patch, Dimensions patch_size, rust::Slice<const std::uint8_t> mask) {
  return match_template_mode(data,size,patch,patch_size,mask,cv::TM_CCOEFF_NORMED);
}
rust::Vec<float> match_template_ccorr(rust::Slice<const std::uint8_t> data, Dimensions size, rust::Slice<const std::uint8_t> patch, Dimensions patch_size, rust::Slice<const std::uint8_t> mask) {
  return match_template_mode(data,size,patch,patch_size,mask,cv::TM_CCORR_NORMED);
}

}
