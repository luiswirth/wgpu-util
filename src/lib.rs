//! wgpu-util is a utility crate for working with wgpu-rs.

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

/// Thin [`wgpu::Buffer`] wrapper with size.
#[derive(Debug)]
pub struct SizedBuffer {
    pub size: wgpu::BufferAddress,
    pub buffer: wgpu::Buffer,
}

impl SizedBuffer {
    pub fn new(size: wgpu::BufferAddress, buffer: wgpu::Buffer) -> Self {
        Self { size, buffer }
    }
}

pub struct BufferResizeWriteDescriptor<'a> {
    pub label: wgpu::Label<'a>,
    pub contents: &'a [u8],
    pub usage: wgpu::BufferUsage,
}

/// Write contents into buffer, resizes if necessary.
///
/// If contents don't fit, creates new buffer with appropriate size.
pub fn resize_write_buffer(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    buffer: SizedBuffer,
    descriptor: &BufferResizeWriteDescriptor,
) -> SizedBuffer {
    let contents_size = descriptor.contents.len() as wgpu::BufferAddress;
    let enough_space = contents_size <= buffer.size;
    if enough_space {
        queue.write_buffer(&buffer.buffer, 0, descriptor.contents);
        buffer
    } else {
        let new = device.create_buffer_init(&BufferInitDescriptor {
            label: descriptor.label,
            contents: descriptor.contents,
            size: None,
            usage: descriptor.usage,
        });
        SizedBuffer::new(contents_size, new)
    }
}

/// A [`wgpu::Buffer`] which dynamically grows based on the contents.
#[derive(Debug)]
pub struct DynamicBuffer {
    raw: wgpu::Buffer,

    label: crate::OwnedLabel,
    size: wgpu::BufferAddress,
    usage: wgpu::BufferUsage,
}

impl DynamicBuffer {
    const RESERVE: bool = true;

    /// Create a new empty buffer.
    pub fn new(device: &wgpu::Device, descriptor: &wgpu::BufferDescriptor) -> Self {
        let raw = device.create_buffer(&descriptor);

        Self {
            raw,
            label: descriptor.label.map(|l| l.to_owned()),
            size: descriptor.size,
            usage: descriptor.usage,
        }
    }

    /// Create a new buffer with contents.
    pub fn new_init(device: &wgpu::Device, descriptor: &crate::BufferInitDescriptor) -> Self {
        let raw = device.create_buffer_init(&descriptor);

        let descriptor = wgpu::BufferDescriptor {
            label: descriptor.label,
            size: descriptor.contents.len() as wgpu::BufferAddress,
            usage: descriptor.usage,
            mapped_at_creation: false,
        };

        Self {
            raw,
            label: descriptor.label.map(|l| l.to_owned()),
            size: descriptor.size,
            usage: descriptor.usage,
        }
    }

    /// Uploads `contents` and resizes the buffer if needed.
    ///
    /// If `contents` fits, uploads using [`wgpu::Queue`], otherwise reallocates and uploads using
    /// [`wgpu::Device`].
    pub fn upload(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, contents: &[u8]) {
        if self.try_upload(queue, contents).is_err() {
            self.upload_by_init(device, contents)
        }
    }

    /// Uploades `data` using [`wgpu::Queue`] without resizing.
    /// Fails if `data` doesn't fit in buffers and returns the size difference.
    pub fn try_upload(
        &mut self,
        queue: &wgpu::Queue,
        contents: &[u8],
    ) -> Result<(), wgpu::BufferAddress> {
        let contents_size = contents.len() as wgpu::BufferAddress;
        if contents_size < self.size {
            queue.write_buffer(&self.raw, 0, contents);
            self.size = contents_size;
            Ok(())
        } else {
            Err(contents_size - self.size)
        }
    }

    /// Allocates a new buffer, replaces the old one and uploades the contents using
    /// [`wgpu::Device`].
    pub fn upload_by_init(&mut self, device: &wgpu::Device, contents: &[u8]) {
        device.create_buffer_init(&crate::BufferInitDescriptor {
            label: self.label.as_deref(),
            contents,
            usage: self.usage,
            size: match Self::RESERVE {
                true => Some(reserve_function(self.size)),
                false => None,
            },
        });
    }

    /// Get a reference to the raw buffer.
    pub fn raw(&self) -> &wgpu::Buffer {
        &self.raw
    }

    /// Convert into raw buffer.
    pub fn into_raw(self) -> wgpu::Buffer {
        self.raw
    }
}

fn reserve_function(last_size: wgpu::BufferAddress) -> wgpu::BufferAddress {
    last_size.pow(2)
}

/// A [`wgpu::Buffer`] Pool (dynamic supply).
#[derive(Debug)]
pub struct BufferPool {
    buffers: Vec<SizedBuffer>,
    occupied: usize,

    label: crate::OwnedLabel,
    usage: wgpu::BufferUsage,
}

impl BufferPool {
    /// Creates a new empty pool.
    pub fn new(descriptor: &BufferPoolDescriptor) -> Self {
        Self {
            buffers: Vec::new(),
            occupied: 0,

            label: descriptor.label.map(|l| l.to_owned()),
            usage: descriptor.usage,
        }
    }

    /// Upload contents to a vacant buffer.
    ///
    /// Returns buffer index.
    /// If no vacant buffer is available, a new one is allocated.
    pub fn upload(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, contents: &[u8]) -> usize {
        if self.occupied < self.buffers.len() {
            let buffer = &mut self.buffers[self.occupied];

            // CCDF
            let label = self.label.as_deref();
            let usage = self.usage;
            replace_with::replace_with_or_abort(buffer, |buffer| {
                resize_write_buffer(
                    device,
                    queue,
                    buffer,
                    &BufferResizeWriteDescriptor {
                        label,
                        contents,
                        usage,
                    },
                )
            });

            self.occupied += 1;
            self.occupied
        } else {
            self.buffers.push(self.create_buffer(device, contents));
            self.occupied += 1;
            self.occupied
        }
    }

    /// Clears pool. Buffers are marked as vacant and reusable.
    pub fn clear(&mut self) {
        self.occupied = 0;
    }

    /// Get occupied buffer by index.
    pub fn get(&self, i: usize) -> Option<&wgpu::Buffer> {
        if i < self.occupied {
            Some(&self.buffers[i].buffer)
        } else {
            None
        }
    }

    /// Get any (occupied and vacant) buffer by index.
    pub fn get_any(&self, i: usize) -> Option<&wgpu::Buffer> {
        self.buffers.get(i).map(|b| &b.buffer)
    }

    /// Pool size (occupied + vacant)
    pub fn size(&self) -> usize {
        self.buffers.len()
    }

    /// Number of occupied buffers
    pub fn occupied(&self) -> usize {
        self.occupied
    }
}

impl BufferPool {
    fn create_buffer(&self, device: &wgpu::Device, contents: &[u8]) -> SizedBuffer {
        let buffer = device.create_buffer_init(&BufferInitDescriptor {
            label: self.label.as_deref(),
            contents,
            usage: self.usage,
            size: None,
        });
        SizedBuffer::new(contents.len() as wgpu::BufferAddress, buffer)
    }
}

/// Descriptor for [`BufferPool`]
pub struct BufferPoolDescriptor<'a> {
    /// Label assigned to all buffers
    pub label: wgpu::Label<'a>,
    /// Usages for all buffer
    pub usage: wgpu::BufferUsage,
}
