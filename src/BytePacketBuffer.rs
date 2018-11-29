use std::io::{Result};
use std::io::{Error, ErrorKind};

pub struct BytePacketBuffer {
    pub buf: [u8; 512],
    pub pos: usize
}

impl BytePacketBuffer {
    pub fn new() -> BytePacketBuffer {
        BytePacketBuffer {
            buf: [0; 512],
            pos: 0
        }
    }

    pub fn pos(&self) -> usize {
        self.pos
    }

    pub fn step(&mut self, steps: usize) -> Result<()> {
        self.pos += steps;
        Ok(())
    }

    fn seek(&mut self, pos: usize) -> Result<()> {
        self.pos = pos;
        Ok(())
    }

    // Read one byte and move forward
    fn read(&mut self) -> Result<u8> {
        if self.pos >= 512 {
            return Err(Error::new(ErrorKind::InvalidInput, "End of buffer"));
        }

        let res = self.buf[self.pos];
        self.pos += 1;

        Ok(res)
    }

    // Get data at a certain position without changing the internal position
    fn get(&mut self, pos: usize) -> Result<u8> {
        if pos >= 512 {
            return Err(Error::new(ErrorKind::InvalidInput, "End of buffer"));
        }

        Ok(self.buf[pos])
    }

    // Set an 8-bit int at some place in the buffer
    pub fn set(&mut self, pos: usize, val: u8) -> Result<()> {
        self.buf[pos] = val;
        Ok(())
    }

    // Set a 16-bit int at some place in the buffer
    pub fn set_u16(&mut self, pos: usize, val: u16) -> Result<()> {
        self.set(pos, (val >> 8) as u8)?;  // ? is the same as wrapping with try!
        self.set(pos, (val & 0xFF) as u8)?;

        Ok(())
    }

    // Get multiple bytes at once without changing the internal position
    pub fn get_range(&mut self, start:usize, len: usize) -> Result<&[u8]> {
        if start + len >= 512 {
            return Err(Error::new(ErrorKind::InvalidInput, "End of buffer"));
        }

        Ok(&self.buf[start..start+len as usize])
    }

    pub fn read_u16(&mut self) -> Result<u16> {
        let res =   ((try!(self.read()) as u16) << 8) | (try!(self.read()) as u16);
        Ok(res)
    }

    pub fn read_u32(&mut self) -> Result<u32> {
        let res =   ((try!(self.read()) as u32) << 24) |
                    ((try!(self.read()) as u32) << 16) |
                    ((try!(self.read()) as u32) << 8)  |
                    ((try!(self.read()) as u32) << 0);
        Ok(res)
    }


    // Read domain names, take labels into consideration
    // Take [3]www[6]google[3]com[0] and append www.google.com to outstr
    pub fn read_qname(&mut self, outstr: &mut String) -> Result<()> {
        // We're going to keep track of the position locally instead of using the struct pos because we're going to jump around
        let mut pos = self.pos();

        // Did we jump?
        let mut jumped = false;

        let mut delim = ""; // Will become a "." after the first iteration
        loop {
            // First fetch the length of the label
            let len = try!(self.get(pos));

            // Jump if the 2 MSB are set
            if (len & 0xC0) == 0xC0 {
                // move past the current label if we did not jump
                if !jumped {
                    try!(self.seek(pos+2));
                }

                // Read another byte, fix the offset and jump by setting pos
                let b2 = try!(self.get(pos+1)) as u16;
                let offset = (((len as u16) ^0xC0) << 8) | b2;
                pos = offset as usize;

                jumped = true;
            } else { // The normal situation: read a single label and append to output

                // Let's move the position beyond our length-byte
                pos += 1;

                // we are done reading
                if len == 0 {
                    break;
                }

                outstr.push_str(delim);

                let str_buffer = try!(self.get_range(pos, len as usize));
                outstr.push_str(&String::from_utf8_lossy(str_buffer).to_lowercase());

                delim = ".";

                // Move to the next label
                pos += len as usize;
            }
        }

        // If we jumped, the position has been changed already
        if !jumped {
            try!(self.seek(pos))
        }

        Ok(())
    }

    fn write(&mut self, val: u8) -> Result<()> {
        if self.pos >= 512 {
            return Err(Error::new(ErrorKind::InvalidInput, "End of buffer"));
        }
        self.buf[self.pos] = val;
        self.pos += 1;
        Ok(())
    }

    pub fn write_u8(&mut self, val: u8) -> Result<()> {
        try!(self.write(val));
        Ok(())
    }

    pub fn write_u16(&mut self, val: u16) -> Result<()> {
        try!(self.write((val >> 8) as u8));
        try!(self.write((val & 0xFF) as u8));

        Ok(())
    }

    pub fn write_u32(&mut self, val: u32) -> Result<()> {
        try!(self.write(((val >> 24) & 0xFF) as u8));
        try!(self.write(((val >> 16) & 0xFF) as u8));
        try!(self.write(((val >> 8)  & 0xFF) as u8));
        try!(self.write(((val >> 0)  & 0xFF) as u8));

        Ok(())
    }

    pub fn write_qname(&mut self, qname: &str) -> Result<()> {
        let split_str = qname.split('.').collect::<Vec<&str>>();

        for label in split_str {
            let len = label.len();
            if len > 0x34 {
                return Err(Error::new(ErrorKind::InvalidInput, "Label exceeds 63 characters"));
            }

            try!(self.write_u8(len as u8));
            for b in label.as_bytes() {
                try!(self.write_u8(*b));
            }
        }

        try!(self.write_u8(0));

        Ok(())
    }



}
