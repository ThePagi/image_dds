use crate::{div_round_up, CompressSurfaceError, DecompressSurfaceError, ImageFormat, Quality};
use half::f16;

// All BCN formats use 4x4 pixel blocks.
const BLOCK_WIDTH: usize = 4;
const BLOCK_HEIGHT: usize = 4;

// Quality modes are optimized for a balance of speed and quality.
impl From<Quality> for intel_tex_2::bc6h::EncodeSettings {
    fn from(value: Quality) -> Self {
        // TODO: Test quality settings and speed for bc6h.
        match value {
            Quality::Fast => intel_tex_2::bc6h::very_fast_settings(),
            Quality::Normal => intel_tex_2::bc6h::basic_settings(),
            Quality::Slow => intel_tex_2::bc6h::slow_settings(),
        }
    }
}

impl From<Quality> for intel_tex_2::bc7::EncodeSettings {
    fn from(value: Quality) -> Self {
        // bc7 has almost imperceptible errors even at ultra_fast
        // 4k rgba ultra fast (2s), very fast (7s), fast (12s)
        match value {
            Quality::Fast => intel_tex_2::bc7::alpha_ultra_fast_settings(),
            Quality::Normal => intel_tex_2::bc7::alpha_very_fast_settings(),
            Quality::Slow => intel_tex_2::bc7::alpha_fast_settings(),
        }
    }
}

// TODO: Make this generic over the pixel type P.
// This can then be implemented for each type for f32 and u8.
pub trait Bcn {
    type CompressedBlock;

    // TODO: change this to -> [P; 16] to support both u8 and f32?
    // Expect all formats to pad to RGBA even if they have fewer channels.
    // Fixing the length should reduce the amount of bounds checking.
    fn decompress_block(block: &Self::CompressedBlock) -> [u8; Rgba::BYTES_PER_BLOCK];

    // TODO: Require the ability to convert &[u8] -> &[P]?
    // Users won't want to convert the data themselves.
    // This trait isn't usable by users, so we can stick to types that work like [u8;4] or [f32;4]
    // TODO: Should the pixel type include the channels like [u8;1] vs [u8;4]?
    fn compress_surface(
        width: u32,
        height: u32,
        rgba8_data: &[u8],
        quality: Quality,
    ) -> Result<Vec<u8>, CompressSurfaceError>;
}

// Allows block types to read and copy buffer data to enforce alignment.
pub trait ReadBlock {
    const SIZE_IN_BYTES: usize;

    fn read_block(data: &[u8], offset: usize) -> Self;
}

// The underlying C/C++ code may cast the array pointer.
// Use a generous alignment to avoid alignment issues.
#[repr(align(8))]
pub struct Block8([u8; 8]);

impl ReadBlock for Block8 {
    const SIZE_IN_BYTES: usize = 8;

    fn read_block(data: &[u8], offset: usize) -> Self {
        Self(data[offset..offset + 8].try_into().unwrap())
    }
}

#[repr(align(16))]
pub struct Block16([u8; 16]);

impl ReadBlock for Block16 {
    const SIZE_IN_BYTES: usize = 16;

    fn read_block(data: &[u8], offset: usize) -> Self {
        Self(data[offset..offset + 16].try_into().unwrap())
    }
}

// TODO: Make a trait for this and make the functions generic?
struct Rgba;
impl Rgba {
    const BYTES_PER_PIXEL: usize = 4;
    const BYTES_PER_BLOCK: usize = 64;
}

pub struct Bc1;
impl Bcn for Bc1 {
    type CompressedBlock = Block8;

    fn decompress_block(block: &Block8) -> [u8; Rgba::BYTES_PER_BLOCK] {
        let mut decompressed = [0u8; BLOCK_WIDTH * BLOCK_HEIGHT * Rgba::BYTES_PER_PIXEL];

        unsafe {
            bcndecode_sys::bcdec_bc1(
                block.0.as_ptr(),
                decompressed.as_mut_ptr(),
                (BLOCK_WIDTH * Rgba::BYTES_PER_PIXEL) as i32,
            );
        }

        decompressed
    }

    fn compress_surface(
        width: u32,
        height: u32,
        rgba8_data: &[u8],
        _: Quality,
    ) -> Result<Vec<u8>, CompressSurfaceError> {
        // RGBA with 4 bytes per pixel.
        let surface = intel_tex_2::RgbaSurface {
            width,
            height,
            stride: width * Rgba::BYTES_PER_PIXEL as u32,
            data: rgba8_data,
        };

        Ok(intel_tex_2::bc1::compress_blocks(&surface))
    }
}

pub struct Bc2;
impl Bcn for Bc2 {
    type CompressedBlock = Block16;

    fn decompress_block(block: &Block16) -> [u8; Rgba::BYTES_PER_BLOCK] {
        let mut decompressed = [0u8; BLOCK_WIDTH * BLOCK_HEIGHT * Rgba::BYTES_PER_PIXEL];

        unsafe {
            bcndecode_sys::bcdec_bc2(
                block.0.as_ptr(),
                decompressed.as_mut_ptr(),
                (BLOCK_WIDTH * Rgba::BYTES_PER_PIXEL) as i32,
            );
        }

        decompressed
    }

    fn compress_surface(
        _width: u32,
        _height: u32,
        _rgba8_data: &[u8],
        _quality: Quality,
    ) -> Result<Vec<u8>, CompressSurfaceError> {
        // TODO: Find an implementation that supports this?
        Err(CompressSurfaceError::UnsupportedFormat {
            format: ImageFormat::BC2Unorm,
        })
    }
}

pub struct Bc3;
impl Bcn for Bc3 {
    type CompressedBlock = Block16;

    fn decompress_block(block: &Block16) -> [u8; Rgba::BYTES_PER_BLOCK] {
        let mut decompressed = [0u8; BLOCK_WIDTH * BLOCK_HEIGHT * Rgba::BYTES_PER_PIXEL];

        unsafe {
            bcndecode_sys::bcdec_bc3(
                block.0.as_ptr(),
                decompressed.as_mut_ptr(),
                (BLOCK_WIDTH * Rgba::BYTES_PER_PIXEL) as i32,
            );
        }

        decompressed
    }

    fn compress_surface(
        width: u32,
        height: u32,
        rgba8_data: &[u8],
        _: Quality,
    ) -> Result<Vec<u8>, CompressSurfaceError> {
        // RGBA with 4 bytes per pixel.
        let surface = intel_tex_2::RgbaSurface {
            width,
            height,
            stride: width * Rgba::BYTES_PER_PIXEL as u32,
            data: rgba8_data,
        };

        Ok(intel_tex_2::bc3::compress_blocks(&surface))
    }
}

pub struct Bc4;
impl Bcn for Bc4 {
    type CompressedBlock = Block8;

    fn decompress_block(block: &Block8) -> [u8; Rgba::BYTES_PER_BLOCK] {
        // BC4 stores grayscale data, so each decompressed pixel is 1 byte.
        let mut decompressed_r = [0u8; BLOCK_WIDTH * BLOCK_HEIGHT];

        unsafe {
            bcndecode_sys::bcdec_bc4(
                block.0.as_ptr(),
                decompressed_r.as_mut_ptr(),
                (BLOCK_WIDTH) as i32,
            );
        }

        // Pad to RGBA with alpha set to white.
        let mut decompressed = [0u8; BLOCK_WIDTH * BLOCK_HEIGHT * Rgba::BYTES_PER_PIXEL];
        for i in 0..(BLOCK_WIDTH * BLOCK_HEIGHT) {
            // It's a convention in some programs display BC4 in the red channel.
            // Use grayscale instead to avoid confusing it with colored data.
            // TODO: Match how channels handled when compressing RGBA data to BC4?
            decompressed[i * Rgba::BYTES_PER_PIXEL] = decompressed_r[i];
            decompressed[i * Rgba::BYTES_PER_PIXEL + 1] = decompressed_r[i];
            decompressed[i * Rgba::BYTES_PER_PIXEL + 2] = decompressed_r[i];
            decompressed[i * Rgba::BYTES_PER_PIXEL + 3] = 255u8;
        }

        decompressed
    }

    fn compress_surface(
        width: u32,
        height: u32,
        rgba8_data: &[u8],
        _: Quality,
    ) -> Result<Vec<u8>, CompressSurfaceError> {
        // RGBA with 4 bytes per pixel.
        let surface = intel_tex_2::RgbaSurface {
            width,
            height,
            stride: width * Rgba::BYTES_PER_PIXEL as u32,
            data: rgba8_data,
        };

        Ok(intel_tex_2::bc4::compress_blocks(&surface))
    }
}

pub struct Bc5;
impl Bcn for Bc5 {
    type CompressedBlock = Block16;

    fn decompress_block(block: &Block16) -> [u8; Rgba::BYTES_PER_BLOCK] {
        // BC5 stores RG data, so each decompressed pixel is 2 bytes.
        let mut decompressed_rg = [0u8; BLOCK_WIDTH * BLOCK_HEIGHT * 2];

        unsafe {
            bcndecode_sys::bcdec_bc5(
                block.0.as_ptr(),
                decompressed_rg.as_mut_ptr(),
                (BLOCK_WIDTH * 2) as i32,
            );
        }

        // Pad to RGBA with alpha set to white.
        let mut decompressed = [0u8; BLOCK_WIDTH * BLOCK_HEIGHT * Rgba::BYTES_PER_PIXEL];
        for i in 0..(BLOCK_WIDTH * BLOCK_HEIGHT) {
            // It's convention to zero the blue channel when decompressing BC5.
            decompressed[i * Rgba::BYTES_PER_PIXEL] = decompressed_rg[i * 2];
            decompressed[i * Rgba::BYTES_PER_PIXEL + 1] = decompressed_rg[i * 2 + 1];
            decompressed[i * Rgba::BYTES_PER_PIXEL + 2] = 0u8;
            decompressed[i * Rgba::BYTES_PER_PIXEL + 3] = 255u8;
        }

        decompressed
    }

    fn compress_surface(
        width: u32,
        height: u32,
        rgba8_data: &[u8],
        _: Quality,
    ) -> Result<Vec<u8>, CompressSurfaceError> {
        // RGBA with 4 bytes per pixel.
        let surface = intel_tex_2::RgbaSurface {
            width,
            height,
            stride: width * Rgba::BYTES_PER_PIXEL as u32,
            data: rgba8_data,
        };

        Ok(intel_tex_2::bc5::compress_blocks(&surface))
    }
}

pub struct Bc6;
impl Bcn for Bc6 {
    type CompressedBlock = Block16;

    fn decompress_block(block: &Block16) -> [u8; Rgba::BYTES_PER_BLOCK] {
        // TODO: signed vs unsigned?
        // TODO: Also support exr or radiance hdr under feature flags?
        // exr or radiance only make sense for bc6

        // BC6H uses half precision floating point data.
        // Convert to single precision since f32 is better supported on CPUs.
        let mut decompressed_rgb = [0f32; BLOCK_WIDTH * BLOCK_HEIGHT * 3];

        unsafe {
            // Cast the pointer to a less strictly aligned type.
            // The pitch is in terms of floats rather than bytes.
            bcndecode_sys::bcdec_bc6h_float(
                block.0.as_ptr(),
                decompressed_rgb.as_mut_ptr() as *mut u8,
                (BLOCK_WIDTH * 3) as i32,
                0,
            );
        }

        // Truncate to clamp to 0 to 255.
        // TODO: Add a separate function that returns floats?
        let float_to_u8 = |x: f32| (x * 255.0) as u8;

        // Pad to RGBA with alpha set to white.
        let mut decompressed = [0u8; BLOCK_WIDTH * BLOCK_HEIGHT * Rgba::BYTES_PER_PIXEL];
        for i in 0..(BLOCK_WIDTH * BLOCK_HEIGHT) {
            decompressed[i * Rgba::BYTES_PER_PIXEL] = float_to_u8(decompressed_rgb[i * 3]);
            decompressed[i * Rgba::BYTES_PER_PIXEL + 1] = float_to_u8(decompressed_rgb[i * 3 + 1]);
            decompressed[i * Rgba::BYTES_PER_PIXEL + 2] = float_to_u8(decompressed_rgb[i * 3 + 2]);
            decompressed[i * Rgba::BYTES_PER_PIXEL + 3] = 255u8;
        }

        decompressed
    }

    fn compress_surface(
        width: u32,
        height: u32,
        rgba8_data: &[u8],
        quality: Quality,
    ) -> Result<Vec<u8>, CompressSurfaceError> {
        // The BC6H encoder expects the data to be in half precision floating point.
        // This differs from the other formats that expect [u8; 4] for each pixel.
        let f16_data: Vec<f16> = rgba8_data
            .iter()
            .map(|v| half::f16::from_f32(*v as f32 / 255.0))
            .collect();

        let surface = intel_tex_2::RgbaSurface {
            width,
            height,
            stride: width * 4 * std::mem::size_of::<f16>() as u32,
            data: bytemuck::cast_slice(&f16_data),
        };

        Ok(intel_tex_2::bc6h::compress_blocks(
            &quality.into(),
            &surface,
        ))
    }
}

pub struct Bc7;
impl Bcn for Bc7 {
    type CompressedBlock = Block16;

    fn decompress_block(block: &Block16) -> [u8; Rgba::BYTES_PER_BLOCK] {
        let mut decompressed = [0u8; BLOCK_WIDTH * BLOCK_HEIGHT * Rgba::BYTES_PER_PIXEL];

        unsafe {
            bcndecode_sys::bcdec_bc7(
                block.0.as_ptr(),
                decompressed.as_mut_ptr(),
                (BLOCK_WIDTH * Rgba::BYTES_PER_PIXEL) as i32,
            );
        }

        decompressed
    }

    fn compress_surface(
        width: u32,
        height: u32,
        rgba8_data: &[u8],
        quality: Quality,
    ) -> Result<Vec<u8>, CompressSurfaceError> {
        // RGBA with 4 bytes per pixel.
        let surface = intel_tex_2::RgbaSurface {
            width,
            height,
            stride: width * Rgba::BYTES_PER_PIXEL as u32,
            data: rgba8_data,
        };

        Ok(intel_tex_2::bc7::compress_blocks(&quality.into(), &surface))
    }
}

// TODO: Make this generic over the pixel type (f32 or u8).
/// Decompress the bytes in `data` to the uncompressed RGBA8 format.
pub fn rgba8_from_bcn<T: Bcn>(
    width: u32,
    height: u32,
    data: &[u8],
) -> Result<Vec<u8>, DecompressSurfaceError>
where
    T::CompressedBlock: ReadBlock,
{
    // TODO: Add an option to parallelize this using rayon?
    // Each block can be decoded independently.

    // Surface dimensions are not validated yet and may cause overflow.
    let expected_size = div_round_up(width as usize, BLOCK_WIDTH)
        .checked_mul(div_round_up(height as usize, BLOCK_HEIGHT))
        .and_then(|v| v.checked_mul(T::CompressedBlock::SIZE_IN_BYTES))
        .ok_or(DecompressSurfaceError::InvalidDimensions { width, height })?;

    // Mipmap dimensions do not need to be multiples of the block dimensions.
    // A mipmap of size 1x1 pixels can still be decoded.
    // Simply checking the data length is sufficient.
    if data.len() < expected_size {
        return Err(DecompressSurfaceError::NotEnoughData {
            expected: expected_size,
            actual: data.len(),
        });
    }

    let mut rgba = vec![0u8; width as usize * height as usize * Rgba::BYTES_PER_PIXEL];

    // BCN formats lay out blocks in row-major order.
    // TODO: calculate x and y using division and mod?
    let mut block_start = 0;
    for y in (0..height).step_by(BLOCK_HEIGHT) {
        for x in (0..width).step_by(BLOCK_WIDTH) {
            // Use a special type to enforce alignment.
            let block = T::CompressedBlock::read_block(data, block_start);
            // TODO: Add rgba8 and rgbaf32 variants for decompress block.
            let decompressed_block = T::decompress_block(&block);

            // TODO: This can be generic over the pixel type to also support float.
            // Each block is 4x4, so we need to update multiple rows.
            put_rgba_block(
                &mut rgba,
                decompressed_block,
                x as usize,
                y as usize,
                width as usize,
            );

            block_start += T::CompressedBlock::SIZE_IN_BYTES;
        }
    }

    Ok(rgba)
}

fn put_rgba_block(
    surface: &mut [u8],
    pixels: [u8; Rgba::BYTES_PER_BLOCK],
    x: usize,
    y: usize,
    width: usize,
) {
    // Place the compressed block into the decompressed surface.
    // For most formats this will be contiguous 4x4 pixel blocks.
    // The data from each block will update 4 rows of the RGBA surface.
    // TODO: Examine the assembly for this.
    // TODO: How to handle grayscale formats like BC4?
    // This should have tunable parameters for Rgba::bytes_per_pixel.
    let bytes_per_row = BLOCK_WIDTH * Rgba::BYTES_PER_PIXEL;

    for row in 0..BLOCK_HEIGHT {
        let surface_index = ((y + row) * width + x) * Rgba::BYTES_PER_PIXEL;
        let pixel_index = row * BLOCK_WIDTH * Rgba::BYTES_PER_PIXEL;
        surface[surface_index..surface_index + bytes_per_row]
            .copy_from_slice(&pixels[pixel_index..pixel_index + bytes_per_row]);
    }
}

/// Compress the uncompressed RGBA8 bytes in `data` to the given format `T`.
pub fn bcn_from_rgba8<T: Bcn>(
    width: u32,
    height: u32,
    data: &[u8],
    quality: Quality,
) -> Result<Vec<u8>, CompressSurfaceError> {
    // TODO: How to handle the zero case?
    if width == 0 || height == 0 {
        return Err(CompressSurfaceError::InvalidDimensions { width, height });
    }

    // Surface dimensions are not validated yet and may cause overflow.
    let expected_size = div_round_up(width as usize, BLOCK_WIDTH)
        .checked_mul(div_round_up(height as usize, BLOCK_HEIGHT))
        .and_then(|v| v.checked_mul(Rgba::BYTES_PER_BLOCK))
        .ok_or(CompressSurfaceError::InvalidDimensions { width, height })?;

    if data.len() < expected_size {
        return Err(CompressSurfaceError::NotEnoughData {
            expected: expected_size,
            actual: data.len(),
        });
    }

    T::compress_surface(width, height, data, quality)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Quality;

    // TODO: Create tests for data length since we can't know what the compressed blocks should be?
    // TODO: Test edge cases and type conversions?
    // TODO: Add tests for validating the input length.
    // TODO: Will compression fail for certain pixel values (test with fuzz tests?)

    fn check_decompress_compressed_bcn<T: Bcn>(rgba: &[u8], quality: Quality)
    where
        T::CompressedBlock: ReadBlock,
    {
        // Compress the data once to introduce some errors.
        let compressed_block = bcn_from_rgba8::<T>(4, 4, &rgba, quality).unwrap();
        let decompressed_block = rgba8_from_bcn::<T>(4, 4, &compressed_block).unwrap();

        // Compressing and decompressing should give back the same data.
        // TODO: Is this guaranteed in general?
        let compressed_block2 = bcn_from_rgba8::<T>(4, 4, &decompressed_block, quality).unwrap();
        let decompressed_block2 = rgba8_from_bcn::<T>(4, 4, &compressed_block2).unwrap();

        assert_eq!(decompressed_block2, decompressed_block);
    }

    #[test]
    fn bc1_decompress_compressed() {
        let rgba = vec![64u8; Rgba::BYTES_PER_BLOCK];
        check_decompress_compressed_bcn::<Bc1>(&rgba, Quality::Fast);
        check_decompress_compressed_bcn::<Bc1>(&rgba, Quality::Normal);
        check_decompress_compressed_bcn::<Bc1>(&rgba, Quality::Slow);
    }

    #[test]
    fn bc2_decompress_compressed() {
        // TODO: Revise this test to check each direction separately.
        // TODO: BC2 compression should return an error.
        let rgba = vec![64u8; Rgba::BYTES_PER_BLOCK];
        check_decompress_compressed_bcn::<Bc2>(&rgba, Quality::Fast);
        check_decompress_compressed_bcn::<Bc2>(&rgba, Quality::Normal);
        check_decompress_compressed_bcn::<Bc2>(&rgba, Quality::Slow);
    }

    #[test]
    fn bc3_decompress_compressed() {
        let rgba = vec![64u8; Rgba::BYTES_PER_BLOCK];
        check_decompress_compressed_bcn::<Bc3>(&rgba, Quality::Fast);
        check_decompress_compressed_bcn::<Bc3>(&rgba, Quality::Normal);
        check_decompress_compressed_bcn::<Bc3>(&rgba, Quality::Slow);
    }

    #[test]
    fn bc4_decompress_compressed() {
        let rgba = vec![64u8; Rgba::BYTES_PER_BLOCK];
        check_decompress_compressed_bcn::<Bc4>(&rgba, Quality::Fast);
        check_decompress_compressed_bcn::<Bc4>(&rgba, Quality::Normal);
        check_decompress_compressed_bcn::<Bc4>(&rgba, Quality::Slow);
    }

    #[test]
    fn bc5_decompress_compressed() {
        let rgba = vec![64u8; Rgba::BYTES_PER_BLOCK];
        check_decompress_compressed_bcn::<Bc5>(&rgba, Quality::Fast);
        check_decompress_compressed_bcn::<Bc5>(&rgba, Quality::Normal);
        check_decompress_compressed_bcn::<Bc5>(&rgba, Quality::Slow);
    }

    #[test]
    fn bc6_decompress_compressed() {
        // TODO: Revise this test to check each direction separately.
        let rgba = vec![64u8; Rgba::BYTES_PER_BLOCK];
        check_decompress_compressed_bcn::<Bc6>(&rgba, Quality::Fast);
        check_decompress_compressed_bcn::<Bc6>(&rgba, Quality::Normal);
        check_decompress_compressed_bcn::<Bc6>(&rgba, Quality::Slow);
    }

    #[test]
    fn bc7_decompress_compressed() {
        let rgba = vec![64u8; Rgba::BYTES_PER_BLOCK];
        check_decompress_compressed_bcn::<Bc7>(&rgba, Quality::Fast);
        check_decompress_compressed_bcn::<Bc7>(&rgba, Quality::Normal);
        check_decompress_compressed_bcn::<Bc7>(&rgba, Quality::Slow);
    }
}
