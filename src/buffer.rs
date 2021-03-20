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
        let raw = crate::create_buffer_init(device, &descriptor);

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

    pub fn upload(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, data: &[u8]) {
        if self.try_upload(queue, data).is_err() {
            self.upload_by_init(device, data)
        }
    }

    pub fn try_upload(
        &mut self,
        queue: &wgpu::Queue,
        data: &[u8],
    ) -> Result<(), wgpu::BufferAddress> {
        let data_len = data.len() as wgpu::BufferAddress;
        if data_len < self.size {
            queue.write_buffer(&self.raw, 0, data);
            self.size = data_len;
            Ok(())
        } else {
            Err(data_len - self.size)
        }
    }

    pub fn upload_by_init(&mut self, device: &wgpu::Device, data: &[u8]) {
        crate::create_buffer_init(
            device,
            &crate::BufferInitDescriptor {
                label: self.label.as_deref(),
                contents: data,
                usage: self.usage,
                size: match Self::RESERVE {
                    true => Some(new_size(self.size)),
                    false => None,
                },
            },
        );
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

// TODO: grow function
fn new_size(last_size: wgpu::BufferAddress) -> wgpu::BufferAddress {
    last_size.pow(2)
}
