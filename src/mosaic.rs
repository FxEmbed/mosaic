/*
 * MIT License
 *
 * Copyright (c) 2022 Antonio32A (antonio32a.com) <~@antonio32a.com>
 *
 * Permission is hereby granted, free of charge, to any person obtaining a copy
 * of this software and associated documentation files (the "Software"), to deal
 * in the Software without restriction, including without limitation the rights
 * to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
 * copies of the Software, and to permit persons to whom the Software is
 * furnished to do so, subject to the following conditions:
 *
 * The above copyright notice and this permission notice shall be included in all
 * copies or substantial portions of the Software.
 *
 * THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
 * IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
 * FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
 * AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
 * LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
 * OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
 * SOFTWARE.
 */

use std::cmp::max;
use std::cmp::Ordering::Equal;
use std::iter::zip;
use std::time::Instant;

use image::{imageops::FilterType, RgbImage};
use tracing::instrument;

use crate::mosaic::fours::build_4_mosaic;
use crate::mosaic::threes::build_3_mosaic;
use crate::mosaic::twos::build_2_mosaic;

mod twos;
mod threes;
mod fours;
mod testutils;

const SPACING_SIZE: u32 = 10;
const MAX_SIZE: u32 = 4000;

pub fn mosaic(mut images: Vec<RgbImage>) -> RgbImage {
    match images.len() {
        2 => {
            let second = images.pop().unwrap();
            let first = images.pop().unwrap();
            build_2_mosaic(first, second)
        }
        3 => {
            let third = images.pop().unwrap();
            let second = images.pop().unwrap();
            let first = images.pop().unwrap();
            build_3_mosaic(first, second, third)
        }
        4 => {
            let fourth = images.pop().unwrap();
            let third = images.pop().unwrap();
            let second = images.pop().unwrap();
            let first = images.pop().unwrap();
            build_4_mosaic(first, second, third, fourth)
        }
        _ => panic!("impossible image length"),
    }
}

fn create_background(size: Size) -> RgbImage {
    RgbImage::from_pixel(size.width, size.height, image::Rgb([0, 0, 0]))
}

fn scale_height_dimension(image_size: Size, other_height: u32) -> Size {
    let scale_factor = image_size.height as f32 / other_height as f32;
    Size {
        width: (image_size.width as f32 / scale_factor).round() as u32,
        height: other_height,
    }
}

fn scale_width_dimension(image_size: Size, other_width: u32) -> Size {
    let scale_factor = image_size.width as f32 / other_width as f32;
    Size {
        width: other_width,
        height: (image_size.height as f32 / scale_factor).round() as u32,
    }
}

fn resize_images(images: Vec<(RgbImage, Size)>) -> Vec<RgbImage> {
    tracing::debug!("resizing {} images", images.len());

    let _span = tracing::Span::current().entered();

    // Resize one image at a time so we never hold multiple full-size decode buffers
    // inside concurrent resizes (each resize temporarily needs source + destination).
    images
        .into_iter()
        .map(|(im, size)| resize_image(im, size))
        .collect()
}

#[instrument(skip(image, size))]
fn resize_image(image: RgbImage, size: Size) -> RgbImage {
    tracing::trace!("starting image resize");

    let start = Instant::now();

    if image.width() != size.width && image.height() != size.height {
        let im = image::imageops::resize(
            &image,
            size.width,
            size.height,
            FilterType::Triangle, // The original uses Lanczos3 but in practice the difference is not visible.
        );

        tracing::debug!(time = start.elapsed().as_millis(), "resized image");

        im
    } else {
        tracing::debug!("image was already acceptable size");

        image
    }
}

#[derive(Clone, Copy, Default)]
pub struct Size {
    pub width: u32,
    pub height: u32,
}

impl Size {
    fn scale(&self, scale_factor: f32) -> Size {
        Size {
            width: (self.width as f32 / scale_factor).round() as u32,
            height: (self.height as f32 / scale_factor).round() as u32,
        }
    }
    fn add(&self, other: Size) -> Size {
        Size {
            width: self.width + other.width,
            height: self.height + other.height,
        }
    }
}

#[derive(Clone, Copy, Default)]
pub struct ImageOffset {
    pub offset: Size,
    pub dimensions: Size,
    pub original_dimensions: Size,
}

impl ImageOffset {
    fn scale(&self, scale_factor: f32) -> ImageOffset {
        ImageOffset {
            offset: self.offset.scale(scale_factor),
            dimensions: self.dimensions.scale(scale_factor),
            original_dimensions: self.original_dimensions,
        }
    }
    fn add_height(&self, height: u32) -> ImageOffset {
        ImageOffset {
            offset: Size {
                width: self.offset.width,
                height: self.offset.height + height,
            },
            dimensions: Size {
                width: self.dimensions.width,
                height: self.dimensions.height,
            },
            original_dimensions: self.original_dimensions,
        }
    }
    fn add_width(&self, width: u32) -> ImageOffset {
        ImageOffset {
            offset: Size {
                width: self.offset.width + width,
                height: self.offset.height,
            },
            dimensions: Size {
                width: self.dimensions.width,
                height: self.dimensions.height,
            },
            original_dimensions: self.original_dimensions,
        }
    }
    fn total_width(&self) -> u32 {
        self.offset.width + self.dimensions.width
    }
    fn total_height(&self) -> u32 {
        self.offset.height + self.dimensions.height
    }
}

trait MosaicDims {
    fn total_size(&self) -> Size;
    fn scale(&self, scale_factor: f32) -> Self;
    fn image_scale_factors(&self) -> Vec<f32>;
    fn min_scale_factor(&self) -> f32;
    fn max_scale_factor(&self) -> f32;
    fn scale_factor_ratio(&self) -> f32;
    fn scale_to_fit(&self) -> Self;
    fn add_height(&self, height: u32) -> Self;
    fn add_width(&self, width: u32) -> Self;

    fn unsquaredness(&self) -> f32 {
        let total_size = self.total_size();
        if total_size.width < total_size.height {
            total_size.height as f32 / total_size.width as f32
        } else {
            total_size.width as f32 / total_size.height as f32
        }
    }
}

#[derive(Clone, Copy)]
pub struct MosaicImageDims<const LEN: usize> {
    images: [ImageOffset; LEN],
}

impl<const LEN: usize> MosaicDims for MosaicImageDims<LEN> {
    fn total_size(&self) -> Size {
        let last = self.images.last().unwrap();
        last.offset.add(last.dimensions)
    }

    fn scale(&self, scale_factor: f32) -> Self {
        let mut new_images = [ImageOffset::default(); LEN];
        for (x, image) in self.images.iter().enumerate() {
            new_images[x] = image.scale(scale_factor);
        }

        MosaicImageDims {
            images: new_images
        }
    }

    fn image_scale_factors(&self) -> Vec<f32> {
        self.images.iter().map(|image| {
            image.dimensions.width as f32 / image.original_dimensions.width as f32
        }).collect()
    }

    fn min_scale_factor(&self) -> f32 {
        *(self.image_scale_factors().iter().min_by(|a, b| {
            a.partial_cmp(&b).unwrap_or(Equal)
        }).unwrap())
    }

    fn max_scale_factor(&self) -> f32 {
        *self.image_scale_factors().iter().max_by(|a, b| {
            a.partial_cmp(&b).unwrap_or(Equal)
        }).unwrap()
    }

    fn scale_factor_ratio(&self) -> f32 {
        self.max_scale_factor() / self.min_scale_factor()
    }

    fn scale_to_fit(&self) -> Self {
        // Scale mosaic so that the smallest image is 1:1 scale
        let mut scaled_mosaic = self.scale(self.min_scale_factor());
        // Scale down to fit into maximum dimensions
        let total_size = scaled_mosaic.total_size();
        let biggest = max(total_size.width, total_size.height);
        if biggest > MAX_SIZE {
            let scale_factor = biggest as f32 / MAX_SIZE as f32;
            scaled_mosaic = scaled_mosaic.scale(scale_factor);
        }
        scaled_mosaic
    }

    fn add_height(&self, height: u32) -> Self {
        let mut new_images = [ImageOffset::default(); LEN];
        for (x, image) in self.images.iter().enumerate() {
            new_images[x] = image.add_height(height);
        }
        MosaicImageDims {
            images: new_images
        }
    }

    fn add_width(&self, width: u32) -> Self {
        let mut new_images = [ImageOffset::default(); LEN];
        for (x, image) in self.images.iter().enumerate() {
            new_images[x] = image.add_width(width);
        }
        MosaicImageDims {
            images: new_images
        }
    }
}

fn best_mosaic<T: MosaicDims + Copy>(mosaics: &[&T]) -> T {
    // Ensure all mosaics have a minimum scaling ratio of 1, and fit within the box
    let scaled_mosaics: Vec<T> = mosaics.iter().map(|mosaic| {
        mosaic.scale_to_fit()
    }).collect();

    // Find the lowest scaling ratio, to discard mosaics with a scaling ratio 50% higher than that
    let min_scale_factor_ratio = scaled_mosaics.iter().map(|mosaic| {
        mosaic.scale_factor_ratio()
    }).min_by(|a, b| {
        a.partial_cmp(&b).unwrap_or(Equal)
    }).unwrap();

    let scale_factor_ratio_cap = min_scale_factor_ratio + 0.5;

    // Then select squarest within 50% of that
    *scaled_mosaics.iter().filter(|mosaic| {
        mosaic.scale_factor_ratio() < scale_factor_ratio_cap
    }).min_by(|mosaic_a, mosaic_b| {
        let ratio_a = mosaic_a.unsquaredness();
        let ratio_b = mosaic_b.unsquaredness();
        ratio_a.partial_cmp(&ratio_b).unwrap_or(Equal)
    }).unwrap()
}


fn build_mosaic<const LEN: usize>(mosaic: MosaicImageDims<LEN>, images: [RgbImage; LEN]) -> RgbImage {
    let resize_args = zip(images, mosaic.images).map(|(image, offset)| {
        (
            image,
            offset.dimensions,
        )
    }).collect();

    let resized = resize_images(resize_args);

    let mut background = create_background(mosaic.total_size());
    for (image, offset) in zip(resized, mosaic.images) {
        image::imageops::overlay(&mut background, &image, offset.offset.width as i64, offset.offset.height as i64);
    }
    background
}

#[cfg(test)]
mod tests {
    use crate::mosaic;
    use crate::mosaic::testutils::{
        BLUE,
        create_with_colour,
        GREEN,
        has_black_horizontal_line,
        has_black_vertical_line,
        has_black_vertical_line_partial,
        is_colour_in_range,
        PURPLE,
        RED,
        save_result,
    };

    #[test]
    fn pick_less_square_option_for_better_scaling_ratio() {
        let top_left = create_with_colour(100, 100, RED);
        let top_right = create_with_colour(300, 100, BLUE);
        let bot_left = create_with_colour(300, 100, GREEN);
        let bot_right = create_with_colour(100, 100, PURPLE);

        let result = mosaic(vec![top_left, top_right, bot_left, bot_right]);

        save_result(&result, "less_square_better_scaling_ratio");
        assert!(is_colour_in_range(0, 0, 100, 100, &result, RED));
        assert!(has_black_vertical_line_partial(105, 0, 100, &result));
        assert!(is_colour_in_range(120, 0, 400, 100, &result, BLUE));
        assert!(has_black_horizontal_line(105, &result));
        assert!(is_colour_in_range(0, 120, 300, 200, &result, GREEN));
        assert!(has_black_vertical_line_partial(305, 120, 200, &result));
        assert!(is_colour_in_range(320, 120, 400, 200, &result, PURPLE));
    }

    #[test]
    fn wont_scale_down_to_match() {
        let left = create_with_colour(100, 200, RED);
        let right = create_with_colour(200, 400, BLUE);

        let result = mosaic(vec![left, right]);

        save_result(&result, "wont_scale_down_to_match");
        assert!(is_colour_in_range(0, 0, 200, 400, &result, RED));
        assert!(has_black_vertical_line(205, &result));
        assert!(is_colour_in_range(220, 0, 400, 400, &result, BLUE));
    }

    #[test]
    fn scale_down_to_fit() {
        let left = create_with_colour(3000, 3300, RED);
        let right = create_with_colour(3000, 3300, BLUE);

        let result = mosaic(vec![left, right]);

        save_result(&result, "scale_down_to_fit");
        assert!(is_colour_in_range(0, 0, 1980, 2180, &result, RED));
        assert!(has_black_vertical_line(2000, &result));
        assert!(is_colour_in_range(2020, 0, 4000, 2180, &result, BLUE));
    }

    #[test]
    fn doesnt_attempt_removed_mosaic() {
        let left_top = create_with_colour(200, 300, RED);
        let left_bot = create_with_colour(200, 300, BLUE);
        let mid = create_with_colour(200, 600, GREEN);
        let right = create_with_colour(200, 600, PURPLE);

        let result = mosaic(vec![left_top, left_bot, mid, right]);

        save_result(&result, "doesnt_attempt_removed_mosaic");
        assert!((result.width() < 590) | (result.width() > 630));
        assert!((result.width() < 590) | (result.width() > 630));
        assert!(has_black_horizontal_line(305, &result));
        assert!(has_black_vertical_line(205, &result));
    }
}
