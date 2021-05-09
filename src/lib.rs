//! wgpu-util is a utility crate for working with wgpu-rs.

pub mod buffer;
pub use buffer::DynamicBuffer;

pub mod pool;
pub use pool::{BufferPool, BufferPoolDescriptor};

/// Owned [`wgpu::Label`].
pub type OwnedLabel = Option<String>;

/// [`wgpu::util::BufferInitDescriptor`] but with an additional `size` field.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct BufferInitDescriptor<'a> {
    /// Debug label of a buffer. This will show up in graphics debuggers for easy identification.
    pub label: wgpu::Label<'a>,
    /// Contents of a buffer on creation.
    pub contents: &'a [u8],
    /// Size of the buffer. Must be at least size of `contents`. If unspecified, the size of `contents` is used.
    pub size: Option<wgpu::BufferAddress>,
    /// Usages of a buffer. If the buffer is used in any way that isn't specified here, the operation
    /// will panic.
    pub usage: wgpu::BufferUsage,
}

/// Extension trait for [`wgpu::Device`].
pub trait DeviceExt {
    /// [`wgpu::util::DeviceExt::create_buffer_init`] but for this [`BufferInitDescriptor`]
    /// (includes size field).
    fn create_buffer_init(&self, desc: &BufferInitDescriptor<'_>) -> wgpu::Buffer;
}

impl DeviceExt for wgpu::Device {
    fn create_buffer_init(&self, descriptor: &BufferInitDescriptor<'_>) -> wgpu::Buffer {
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
        let padded_size =
            ((unpadded_size + align_mask) & !align_mask).max(wgpu::COPY_BUFFER_ALIGNMENT);

        let normal_descriptor = wgpu::BufferDescriptor {
            label: descriptor.label,
            size: padded_size,
            usage: descriptor.usage,
            mapped_at_creation: true,
        };

        let buffer = self.create_buffer(&normal_descriptor);
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
}

/// [`wgpu::Buffer`] wrapper with size.
pub struct SizedBuffer {
    pub size: wgpu::BufferAddress,
    pub buffer: wgpu::Buffer,
}

impl SizedBuffer {
    pub fn new(size: wgpu::BufferAddress, buffer: wgpu::Buffer) -> Self {
        Self { size, buffer }
    }
}

// Private

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
        let new = device.create_buffer_init(&BufferInitDescriptor {
            label: descriptor.label,
            contents: descriptor.data,
            size: None,
            usage: descriptor.usage,
        });
        SizedBuffer::new(data_size, new)
    }
}
