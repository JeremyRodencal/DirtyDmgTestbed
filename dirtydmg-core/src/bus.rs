use std::rc::Rc;
use std::cell::RefCell;

// A trait that lets data be written and read from an address.
pub trait BusRW{
    fn bus_write8(&mut self, addr:usize, value:u8);
    fn bus_read8(&mut self, addr:usize) -> u8;

    /// Default 16bit read implementation made of 2 8 bit reads.
    fn bus_read16(&mut self, addr: usize) -> u16 {
        let high_byte = self.bus_read8(addr + 1) as u16;
        (high_byte << 8) | (self.bus_read8(addr) as u16)
    }

    /// Default 16bit write implementation made of 2 8 bit writes.
    fn bus_write16(&mut self, addr: usize, value: u16){
        self.bus_write8(addr, value as u8);
        self.bus_write8(addr + 1, (value >> 8) as u8);
    }
}

// An item to encapsulate a device attached to a bus.
pub struct BusItem {
    start_addr: usize,
    end_addr: usize,
    item: Rc<RefCell<dyn BusRW>>
}

impl BusItem {

    pub fn new(start_addr: usize, end_addr: usize, item: Rc<RefCell<dyn BusRW>>) -> BusItem
    {
        BusItem {
            start_addr,
            end_addr,
            item: item.clone()
        }
    }

    pub fn in_range(&self, addr:usize) -> bool
    {
        self.start_addr <= addr && self.end_addr >= addr
    }

    pub fn bus_write8(&self, addr: usize, value: u8)
    {
        self.item.as_ref().borrow_mut().bus_write8(addr, value);
    }

    pub fn bus_write16(&self, addr: usize, value: u16)
    {
        self.item.as_ref().borrow_mut().bus_write16(addr, value);
    }

    pub fn bus_read8(&self, addr: usize) -> u8
    {
        return self.item.as_ref().borrow_mut().bus_read8(addr);
    }

    pub fn bus_read16(&self, addr:usize) -> u16
    {
        return self.item.as_ref().borrow_mut().bus_read16(addr);
    }
}

pub struct Bus {
    members: Vec<BusItem>
}

impl Bus {
    pub fn new()->Bus {
        Bus {
            members: Vec::new()
        }
    }

    fn get_item(&self, addr:usize)->Option<&BusItem>{
        self.members.iter().find(|&x|x.in_range(addr))
    }

    pub fn add_item(&mut self, item:BusItem)
    {
        self.members.push(item);
    }
}

impl Default for Bus {
    fn default() -> Self {
        Self::new()
    }
}

impl BusRW for Bus {

    fn bus_write8(&mut self, addr:usize, value:u8)
    {
        if let Some(item) = self.get_item(addr) {
            item.bus_write8(addr, value);
        }
    }

    fn bus_read8(&mut self, addr:usize) -> u8
    {
        match self.get_item(addr){
            Some(x) => x.bus_read8(addr),
            None => 0xff
        }
    }
}
