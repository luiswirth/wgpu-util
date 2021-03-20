use crate::SizedBuffer;

use replace_with::replace_with;

pub struct BufferPool {
    buffers: Vec<SizedBuffer>,
    occupied: usize,

    label: crate::OwnedLabel,
    usage: wgpu::BufferUsage,
}

impl BufferPool {
    pub fn new(descriptor: &BufferPoolDescriptor) -> Self {
        Self {
            buffers: Vec::new(),
            occupied: 0,

            label: descriptor.label.map(|l| l.to_owned()),
            usage: descriptor.usage,
        }
    }

    pub fn get(&self, i: usize) -> Option<&wgpu::Buffer> {
        self.buffers.get(i).map(|b| &b.buffer)
    }

    pub fn upload(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, data: &[u8]) -> usize {
        if self.occupied < self.buffers.len() {
            let buffer = &mut self.buffers[self.occupied];

            // CCDF
            let label = self.label.as_deref();
            let usage = self.usage;
            replace_with(
                buffer,
                || unreachable!(),
                |buffer| {
                    crate::write(
                        device,
                        queue,
                        buffer,
                        &crate::WriteDescriptor { label, data, usage },
                    )
                },
            );

            self.occupied += 1;
            self.occupied
        } else {
            self.buffers.push(self.create(device, data));
            self.occupied += 1;
            self.occupied
        }
    }

    pub fn clear(&mut self) {
        self.occupied = 0;
    }

    fn create(&self, device: &wgpu::Device, data: &[u8]) -> SizedBuffer {
        let buffer = crate::create_buffer_init(
            &device,
            &crate::BufferInitDescriptor {
                label: self.label.as_deref(),
                contents: data,
                usage: self.usage,
                size: None,
            },
        );
        SizedBuffer::new(data.len() as wgpu::BufferAddress, buffer)
    }
}

pub struct BufferPoolDescriptor<'a> {
    /// Label assigned to all buffers
    pub label: wgpu::Label<'a>,
    /// Usages for all buffer
    pub usage: wgpu::BufferUsage,
}

pub struct BufferHandle {
    pub index: usize,
}
