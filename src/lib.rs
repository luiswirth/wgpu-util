//! wgpu-util is a utility crate for working with wgpu-rs.
//! It provides a wrapper around [`wgpu::Buffer`] called [`DynamicBuffer`], which dynamically
//! allocates new memory.
//! And a [`BufferPool`] which is a dynamic supply for automatically resizing [`wgpu::Buffer`]s.

pub mod buffer;
pub use buffer::*;

pub mod pool;
pub use pool::*;

pub type OwnedLabel = Option<String>;

/// Describes a [`DynamicBuffer`] when allocating.
///
/// The same as [`wgpu::util::BufferInitDescriptor`] but with an additional `size` field.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct BufferInitDescriptor<'a> {
    /// Debug label of a buffer. This will show up in graphics debuggers for easy identification.
    pub label: wgpu::Label<'a>,
    /// Contents of a buffer on creation.
    pub contents: &'a [u8],
    /// Size of the buffer. Must be at least size of `contents`. If unspecified, the size of `contents` are used.
    pub size: Option<wgpu::BufferAddress>,
    /// Usages of a buffer. If the buffer is used in any way that isn't specified here, the operation
    /// will panic.
    pub usage: wgpu::BufferUsage,
}

pub fn create_buffer_init(
    device: &wgpu::Device,
    descriptor: &BufferInitDescriptor<'_>,
) -> wgpu::Buffer {
    let unpadded_size = {
        let contents_size = descriptor.contents.len() as wgpu::BufferAddress;
        match descriptor.size {
            None => contents_size,
            Some(specified_size) => {
                assert!(
                    specified_size >= contents_size,
                    "specified size must at least be size of contents"
                );
                specified_size
            }
        }
    };

    // Valid vulkan usage is
    // 1. buffer size must be a multiple of COPY_BUFFER_ALIGNMENT.
    // 2. buffer size must be greater than 0.
    // Therefore we round the value up to the nearest multiple, and ensure it's at least COPY_BUFFER_ALIGNMENT.
    let align_mask = wgpu::COPY_BUFFER_ALIGNMENT - 1;
    let padded_size = ((unpadded_size + align_mask) & !align_mask).max(wgpu::COPY_BUFFER_ALIGNMENT);

    let normal_descriptor = wgpu::BufferDescriptor {
        label: descriptor.label,
        size: padded_size,
        usage: descriptor.usage,
        mapped_at_creation: true,
    };

    let buffer = device.create_buffer(&normal_descriptor);
    {
        let mut slice = buffer.slice(..).get_mapped_range_mut();
        slice[0..unpadded_size as usize].copy_from_slice(descriptor.contents);

        for i in unpadded_size..padded_size {
            slice[i as usize] = 0;
        }
    }
    buffer.unmap();
    buffer
}

pub struct SizedBuffer {
    pub size: wgpu::BufferAddress,
    pub buffer: wgpu::Buffer,
}

impl SizedBuffer {
    pub fn new(size: wgpu::BufferAddress, buffer: wgpu::Buffer) -> Self {
        Self { size, buffer }
    }
}

pub(crate) struct WriteDescriptor<'a> {
    pub label: wgpu::Label<'a>,
    pub data: &'a [u8],
    pub usage: wgpu::BufferUsage,
}

#[inline]
pub(crate) fn write(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    buffer: SizedBuffer,
    descriptor: &WriteDescriptor,
) -> SizedBuffer {
    let data_size = descriptor.data.len() as wgpu::BufferAddress;
    let enough_space = data_size <= buffer.size;
    if enough_space {
        queue.write_buffer(&buffer.buffer, 0, descriptor.data);
        buffer
    } else {
        let new = create_buffer_init(
            device,
            &BufferInitDescriptor {
                label: descriptor.label,
                contents: descriptor.data,
                size: None,
                usage: descriptor.usage,
            },
        );
        SizedBuffer::new(data_size, new)
    }
}
