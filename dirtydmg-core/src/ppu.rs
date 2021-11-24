use crate::bus::{BusRW};
use crate::interrupt::InterruptStatus;

/// Overall size of the ram block used for tile sets.
const TILESET_RAM:usize = 0x1800;
/// The total number of tiles in the tile sets.
const TILESET_COUNT:usize = 0x180;
/// The starting RAM address of the tile sets.
const TILESET_START_ADDRESS:usize = 0x8000;
/// The ending RAM address of the tile sets.
const TILESET_END_ADDRESS:usize = TILESET_START_ADDRESS + TILESET_RAM - 1;

/// The width and height of a tile in pixels
const TILE_DIMENSION:usize = 8;
/// The number of bytes in an individual tile
const TILE_SIZE:usize = 16;

/// A tile map is 32x32 tiles.
const TILEMAP_DIMENSION: usize = 32;
/// The number of tiles in a tile map.
const TILEMAP_ITEM_COUNT: usize = TILEMAP_DIMENSION * TILEMAP_DIMENSION;
/// The number of tile maps on the system
const TILEMAPS_COUNT: usize = 2;
/// The ram size of all tile maps combined.
const TILEMAPS_SIZE: usize = TILEMAPS_COUNT * TILEMAP_ITEM_COUNT;
/// The starting address of all tile set data
const TILEMAP_START_ADDRESS:usize = 0x9800;
const TILEMAP_A_START_ADDRESS: usize = 0x9800;
const TILEMAP_A_END_ADDRESS: usize = 0x9BFF;
const TILEMAP_B_START_ADDRESS: usize = 0x9C00;
const TILEMAP_B_END_ADDRESS: usize = 0x9FFF;
/// The ending address of tile set data
const TILEMAP_END_ADDRESS:usize = 0x9FFF;

/// The number of tiles in OAM memory.
const OAM_SPRITE_COUNT: usize = 40;
/// The size of an individual sprite in memory.
const OAM_SPRITE_SIZE: usize = 4;
/// The amount of ram used for OAM.
const OAM_RAM_SIZE:usize = OAM_SPRITE_COUNT * OAM_SPRITE_SIZE;
const OAM_START_ADDRESS:usize = 0xFE00;
const OAM_END_ADDRESS:usize = OAM_START_ADDRESS + OAM_RAM_SIZE;

// LCD Registers
const LCDC_ADDRESS:usize = 0xFF40;
const LCDS_ADDRESS:usize = 0xFF41;

// Sroll registers
const SCY_ADDRESS:usize = 0xFF42;
const SCX_ADDRESS:usize = 0xFF43;
const LY_ADDRESS:usize = 0xFF44;
const LYC_ADDRES:usize = 0xFF45;
const WY_ADDRESS:usize = 0xFF4A;
const WX_ADDRESS:usize = 0xFF4B;

const OAM_DMA_REGISTER_ADDRESS:usize = 0xFF46;

// Palette registers
const BG_PALETTE_ADDRESS:usize = 0xFF47;
const OBJ_PALETTE1_ADDRESS:usize = 0xFF48;
const OBJ_PALETTE2_ADDRESS:usize = 0xFF49;


#[derive(Clone, Copy)]
/// Structure to hold tile pixel data in an easily accessable format.
struct Tile {
    /// tile pixel data.
    pixel: [[u8;TILE_DIMENSION];TILE_DIMENSION]
}

impl Tile {
    /// Reads pixel from the tile set.
    fn read_pixel(&self, x: u8, y: u8) -> u8
    {
        self.pixel[y as usize][x as usize]
    }

    /// Writes a pixel in the tile set.
    fn write_pixel(&mut self, x: u8, y: u8, value: u8)
    {
        self.pixel[y as usize][x as usize] = value;
    }

    // Updates a row of a tile with either msb or lsb pixel data.
    pub fn update_row(&mut self, mut data:u8, y:usize, msb:bool)
    {
        let shift = if msb {1} else {0};
        let clear_mask = !(1<<shift);

        // Pixels are stored such that the leftmost pixel is the MSB and rightmost pixel is the LSB
        // of the data byte.
        // This loop starts with the rightmost pixel, and works its way left.
        for x in (0..8).rev(){
            self.pixel[y][x] &= clear_mask;
            self.pixel[y][x] |= (data & 0x01) << shift;
            data >>= 1;
        }
    }

    fn new() -> Tile{
        Tile{
            pixel: [[0;TILE_DIMENSION];TILE_DIMENSION]
        }
    }
}

struct Palette{
    pub table: [u8;4],
    pub raw: u8,
}

impl Palette {
    /// Constructs a new blank palette.
    fn new() -> Palette{
        Palette{
            table: [0;4],
            raw: 0
        }
    }

    /// Updates the palette with new encoded data.
    /// 
    /// This will update both the raw and table values.
    fn update(&mut self, mut raw:u8){
        // Update the raw value
        self.raw = raw;
        for i in (0..4){
            self.table[i] = raw & 0b11;
            raw >>= 2;
        }
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
struct OamSprite{
    pub ypos: u8,
    pub xpos: u8,
    pub tile: u8,

    //These fields are all packed into a single byte.
    pub behind_background: bool,
    pub xflip: bool,
    pub yflip: bool,
    pub palette: bool,
}

impl OamSprite {
    const PALLET_ATTRIB_MASK:u8 = 0b0001_0000;
    const XFLIP_ATTRIB_MASK:u8 =  0b0010_0000;
    const YFLIP_ATTRIB_MASK:u8 =  0b0100_0000;
    const BG_PRIORITY_ATTRIB_MASK:u8 = 0b1000_0000;

    fn new() -> OamSprite{
        OamSprite{
            ypos: 0,
            xpos: 0,
            tile: 0,
            behind_background: false,
            xflip: false,
            yflip: false,
            palette: false
        }
    }

    fn set_attrib_byte(&mut self, data:u8)
    {
        self.behind_background = data & OamSprite::BG_PRIORITY_ATTRIB_MASK != 0;
        self.xflip = data & OamSprite::XFLIP_ATTRIB_MASK != 0;
        self.yflip = data & OamSprite::YFLIP_ATTRIB_MASK != 0;
        self.palette = data & OamSprite::PALLET_ATTRIB_MASK != 0;
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Mode{
    HBLANK = 0,
    VBLANK = 1,
    SPRITE_SEARCH = 2,
    LCD_TRANSFER = 3
}

pub struct PPU {
    /// Raw tile data stored in the origial gameboy format.
    tile_data: [u8;TILESET_RAM],
    /// Nicely broken out pixel verions of the raw tile data.
    tiles: [Tile;TILESET_COUNT],

    /// tilemap data.
    tilemaps: [u8;TILEMAPS_SIZE],

    /// Decoded sprite object data
    sprites: [OamSprite; OAM_SPRITE_COUNT],
    /// Raw OAM data.
    sprite_data: [u8;OAM_RAM_SIZE],

    // LCDC register
    lcdc: u8,
    lcd_enabled: bool,
    window_tiles_high: bool,    //True if window tiles are in the upper bank.
    window_enabled: bool,
    bg_window_signed_addressing: bool, // True if bg and window tiles are using the signed addressing method
    bg_tiles_high: bool,        // True if the window tiles are in the upper bank.
    obj_double_sprites: bool,   // True if sprites are 8x16, false if 8x8.
    obj_enabled: bool,
    bg_window_enable: bool,     // True if the background/window are enabled.

    // LCD status register
    lcds: u8,
    // interrupt sources
    line_compare_is: bool,
    mode2_is: bool,
    mode1_is: bool,
    mode0_is: bool, 
    line_compare: bool,
    mode: Mode,

    // scroll registers
    scroll_y: u8,
    scroll_x: u8,
    line_y: u8,
    line_compare_value: u8,
    window_y: u8,
    window_x: u8,

    // Pallet 
    bg_palette: Palette,
    obj_palette1: Palette,
    obj_palette2: Palette,

    // OAM DMA
    oam_dma_ticks: u8,
    oam_dma_src: u16,

    // Misc State tracking.
    tick_counter: u16
}

impl PPU {
    // LCDC bit masks.
    const LCDC_ENABLE_MASK: u8                      = 0b1000_0000;
    const LCDC_WINDOW_TILE_MAP_MASK: u8             = 0b0100_0000;
    const LCDC_WINDOW_DISPLAY_ENABLE_MASK: u8       = 0b0010_0000;
    const LCDC_BG_WINDOW_TILE_MAP_SELECT_MASK: u8   = 0b0001_0000;
    const LCDC_BG_TILE_MAP_SELECT_MASK: u8          = 0b0000_1000;
    const LCDC_OBJ_SIZE_MASK: u8                    = 0b0000_0100;
    const LCDC_OBJ_DISPLAY_ENABLE_MASK: u8          = 0b0000_0010;
    const LCDC_BG_WINDOW_PRIORITY_MASK: u8          = 0b0000_0001;

    const LCDS_LINE_CMP_IS_MASK: u8 = 1<<6;
    const LCDS_MODE2_IS_MASK: u8 =    1<<5;
    const LCDS_MODE1_IS_MASK: u8 =    1<<4;
    const LCDS_MODE0_IS_MASK: u8 =    1<<3;

    const OAM_DMA_TRANSFER_TICKS: u8 = 160; // In cpu ticks or "T" cycles.
    
    const LCD_TICKS_PER_LINE: u16 = 456;
    const LCD_LINE_VBLANK_START: u8 = 144;
    const LCD_LINE_VBLANK_END: u8 = 153;

    /// Checks if a DMA transfer is currently executing.
    fn dma_active(&self) -> bool{
        return self.oam_dma_ticks != 0;
    }

    fn update_dma(&mut self, ticks:u16, bus:&mut impl BusRW){
        // If there is a DMA transfer in progress
        if self.oam_dma_ticks > 0 {
            // If the transfer was just initiated.
            if self.oam_dma_ticks == PPU::OAM_DMA_TRANSFER_TICKS{
                self.dma_transfer(bus);
            }

            // Update the number of remaining DMA ticks.
            if self.oam_dma_ticks as u16 > ticks{
                self.oam_dma_ticks -= ticks as u8;
            } else {
                self.oam_dma_ticks = 0;
            }
        }
    }

    fn draw_line(&mut self) {
        // TODO actually draw a line.
    }

    /// # Executes the specified number of clock ticks.
    pub fn execute_ticks(&mut self, ticks:u16, bus:&mut impl BusRW, is: &mut InterruptStatus){
        self.update_dma(ticks, bus);

        // TODO this is really, Really, REALLY wildly inacurate.
        if self.lcd_enabled{
            self.tick_counter += ticks;

            // If the line has expired
            if self.tick_counter >= PPU::LCD_TICKS_PER_LINE {

                // correct the tick count and increment the line count.
                self.tick_counter -= PPU::LCD_TICKS_PER_LINE;
                self.line_y += 1;
                self.line_compare = self.line_compare_value == self.line_y;
                if self.line_compare {
                    if self.line_compare_is {
                        is.request_lcdstat();
                    }
                }

                // if start of vblank
                if self.line_y == PPU::LCD_LINE_VBLANK_START {
                    // Set the mode
                    self.mode = Mode::VBLANK;

                    // Trigger interrupts
                    is.request_vblank();
                    if self.mode1_is {
                        is.request_lcdstat();
                    }
                }

                // start of new frame.
                if self.line_y > PPU::LCD_LINE_VBLANK_END {
                    self.line_y = 0;
                    self.mode = Mode::SPRITE_SEARCH;
                }

                self.draw_line();
            }

            // If we are not in vblank
            if self.line_y < PPU::LCD_LINE_VBLANK_START {
                
                let new_mode = match self.tick_counter {
                    // Mode 2 - OAM_SCAN
                    0..=79 => {
                        Mode::SPRITE_SEARCH
                    }
                    // Mode 3 - Drawing Pixels
                    80..=251 => {
                        Mode::LCD_TRANSFER
                    }
                    // Mode 0 - HBLANK
                    _ => {
                        Mode::HBLANK
                    }
                };

                // If there was a mode change, set any interrupts.
                if new_mode != self.mode {
                    self.mode = new_mode;
                    match new_mode {
                        Mode::SPRITE_SEARCH => {
                            if self.mode2_is{
                                is.request_lcdstat();
                            }
                        }
                        Mode::HBLANK => {
                            if self.mode0_is {
                                is.request_lcdstat();
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    pub fn new() -> PPU {
        let blank_tile = Tile::new();
        let default_sprite = OamSprite::new();
        PPU {
            tile_data: [0;TILESET_RAM],
            tiles: [blank_tile;TILESET_COUNT],
            tilemaps:[0;TILEMAPS_SIZE],
            sprites: [default_sprite;OAM_SPRITE_COUNT],
            sprite_data: [0;OAM_RAM_SIZE],
            lcdc: 0,
            lcd_enabled: false,
            obj_double_sprites: false,
            obj_enabled: false,
            bg_tiles_high: false,
            bg_window_enable: false,
            bg_window_signed_addressing: false,
            window_enabled: false,
            window_tiles_high: false,
            lcds: 0,
            scroll_y: 0,
            scroll_x: 0,
            line_y: 0,
            line_compare_value: 0,
            window_y: 0,
            window_x: 0,
            line_compare_is: false,
            mode2_is: false,
            mode1_is: false,
            mode0_is: false, 
            line_compare: false,
            mode: Mode::HBLANK,
            bg_palette: Palette::new(),
            obj_palette1: Palette::new(),
            obj_palette2: Palette::new(),
            oam_dma_src: 0,
            oam_dma_ticks: 0,
            tick_counter: 0,
        }
    }

    fn tile_write(&mut self, data:u8, addr:usize)
    {
        let index = (addr - TILESET_START_ADDRESS) / TILE_SIZE;
        let y = (addr>>1) & 0x7;
        let msb =  (addr & 0x01) != 0;

        // Update the raw copy of the data
        self.tile_data[addr - TILESET_START_ADDRESS] = data;
        // Update the tile data.
        self.tiles[index].update_row(data, y, msb);
    }

    fn sprite_write(&mut self, data:u8, addr:usize) {
        let index = (addr - OAM_START_ADDRESS) / OAM_SPRITE_SIZE;
        let field = addr & 0b11;

        // Update the sprite attributes
        match field {
            0 => {self.sprites[index].ypos = data;},
            1 => {self.sprites[index].xpos = data;},
            2 => {self.sprites[index].tile = data;},
            3 => {self.sprites[index].set_attrib_byte(data);},
            _ => {panic!("THIS IS A BUG! invalid sprite field write");}
        }

        // Save the raw sprite data.
        self.sprite_data[addr - OAM_START_ADDRESS] = data;
    }

    fn lcdc_write(&mut self, data:u8) {
        // Save the new LCDC value
        self.lcdc = data;
        
        self.lcd_enabled = data & PPU::LCDC_ENABLE_MASK != 0;
        self.window_tiles_high = data & PPU::LCDC_WINDOW_TILE_MAP_MASK != 0;
        self.window_enabled = data & PPU::LCDC_WINDOW_DISPLAY_ENABLE_MASK != 0;
        self.bg_window_signed_addressing = data & PPU::LCDC_BG_WINDOW_TILE_MAP_SELECT_MASK != 0;
        self.bg_tiles_high = data & PPU::LCDC_BG_TILE_MAP_SELECT_MASK != 0;
        self.obj_double_sprites = data & PPU::LCDC_OBJ_SIZE_MASK != 0;
        self.obj_enabled = data & PPU::LCDC_OBJ_DISPLAY_ENABLE_MASK != 0;
        self.bg_window_enable = data & PPU::LCDC_BG_WINDOW_PRIORITY_MASK != 0;
    }

    fn lcds_write(&mut self, data:u8) {
        // Only these bits are writable
        self.line_compare_is = data & PPU::LCDS_LINE_CMP_IS_MASK != 0;
        self.mode2_is = data & PPU::LCDS_MODE2_IS_MASK != 0;
        self.mode1_is = data & PPU::LCDS_MODE1_IS_MASK != 0;
        self.mode0_is = data & PPU::LCDS_MODE0_IS_MASK != 0;
    }

    fn lcds_read(&mut self) -> u8 {
        // Reassemble the LCDS value one bit at a time, starting with the msb.
        let mut value = 0;
        value |= self.line_compare_is as u8;
        value <<= 1;
        value |= self.mode2_is as u8;
        value <<= 1;
        value |= self.mode1_is as u8;
        value <<= 1;
        value |= self.mode0_is as u8;
        value <<= 1;
        value |= self.line_compare as u8;
        value <<= 2;
        value |= self.mode as u8;
        value
    }

    /// # Stage a DMA transfer
    /// 
    /// The actual transfer will not be executed until the next set of PPU
    /// updates.
    fn dma_start(&mut self, target: u8) {
        self.oam_dma_src = (target as u16) << 8;
        self.oam_dma_ticks = PPU::OAM_DMA_TRANSFER_TICKS;
    }

    /// #Executes the DMA memory transfer.
    /// 
    /// This is not done tick by tick, but in one large operation. It should 
    /// not have any negative effects, since the source area and target area
    /// will be blocked during the transfer.
    fn dma_transfer(&mut self, bus:&mut impl BusRW){
        let address = self.oam_dma_src as usize;
        for x in 0..OAM_RAM_SIZE{
            self.sprite_write(
                bus.bus_read8(address + x), 
                OAM_START_ADDRESS + x);
        }
    }
}

impl BusRW for PPU{
    fn bus_read8(&mut self, addr: usize)-> u8{
        match addr {
            // Tile data read
            TILESET_START_ADDRESS..=TILESET_END_ADDRESS => {
                self.tile_data[addr-TILESET_START_ADDRESS]
            },

            // Tile map read
            TILEMAP_START_ADDRESS..=TILEMAP_END_ADDRESS => {
                self.tilemaps[addr-TILEMAP_START_ADDRESS]
            },

            // Object attribute memory read
            OAM_START_ADDRESS..=OAM_END_ADDRESS => {
                self.sprite_data[addr - OAM_START_ADDRESS]
            },

            // Individual registers
            LCDC_ADDRESS => {self.lcdc}
            LCDS_ADDRESS => {self.lcds_read()}
            SCY_ADDRESS => {self.scroll_y}
            SCX_ADDRESS => {self.scroll_x}
            LY_ADDRESS => {self.line_y}
            LYC_ADDRES => {self.line_compare_value}
            WY_ADDRESS => {self.window_y}
            WX_ADDRESS => {self.window_x}
            BG_PALETTE_ADDRESS => {self.bg_palette.raw}
            OBJ_PALETTE1_ADDRESS => {self.obj_palette1.raw}
            OBJ_PALETTE2_ADDRESS => {self.obj_palette2.raw}
            OAM_DMA_REGISTER_ADDRESS => {(self.oam_dma_src>>8) as u8}

            // Unknown read address.
            _ => {
                panic!("Unknown PPU read at address: 0x{:4X}", addr)
            }
        }
    }

    fn bus_write8(&mut self, addr: usize, value: u8){
        // TODO
        match addr {
            // Tile data write
            TILESET_START_ADDRESS..=TILESET_END_ADDRESS => {
                self.tile_write(value, addr);
            }

            // Tile map write
            TILEMAP_START_ADDRESS..=TILEMAP_END_ADDRESS => {
                self.tilemaps[addr-TILEMAP_START_ADDRESS] = value;
            },

            // OAM memory write.
            OAM_START_ADDRESS..=OAM_END_ADDRESS => {
                self.sprite_write(value, addr);
            }

            // LCD control register
            LCDC_ADDRESS => {
                self.lcdc_write(value);
            }

            // LCD status register
            LCDS_ADDRESS => {
                self.lcds_write(value);
            }

            // Scroll and compare registers.
            SCY_ADDRESS => {self.scroll_y = value;}
            SCX_ADDRESS => {self.scroll_x = value;}
            LY_ADDRESS => {/*Dead Write*/}
            LYC_ADDRES => {self.line_compare_value = value;}
            WY_ADDRESS => {self.window_y = value;}
            WX_ADDRESS => {self.window_x = value;}
            BG_PALETTE_ADDRESS => {self.bg_palette.update(value);}
            OBJ_PALETTE1_ADDRESS => {self.obj_palette1.update(value);}
            OBJ_PALETTE2_ADDRESS => {self.obj_palette2.update(value);}
            OAM_DMA_REGISTER_ADDRESS => {self.dma_start(value);}

            // Unknown address.
            _ => {
                panic!("Unknown PPU write at address 0x{:4X}", addr);
            }
        }
    }

    fn bus_read16(&mut self, addr: usize) -> u16 {
        let mut value = self.bus_read8(addr + 1) as u16;
        value <<= 8;
        value |= self.bus_read8(addr) as u16;
        return value;
    }

    fn bus_write16(&mut self, addr: usize, value: u16){
        self.bus_write8(addr, value as u8 & 0xFF);
        self.bus_write8(addr + 1, (value >> 8) as u8);
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::ram::Ram;

    #[test]
    fn test_tile_write()
    {
        // Test data
        let tile_data = [0x7C, 0x7C, 0x00, 0xC6, 0xC6, 0x00, 0x00, 0xFE, 0xC6, 0xC6, 0x00, 0xC6, 0xC6, 0x00, 0x00, 0x00];
        let expected_tile = Tile{
            pixel: [
                [0, 3, 3, 3, 3, 3, 0, 0],
                [2, 2, 0, 0, 0, 2, 2, 0],
                [1, 1, 0, 0, 0, 1, 1, 0],
                [2, 2, 2, 2, 2, 2, 2, 0],
                [3, 3, 0, 0, 0, 3, 3, 0],
                [2, 2, 0, 0, 0, 2, 2, 0],
                [1, 1, 0, 0, 0, 1, 1, 0],
                [0, 0, 0, 0, 0, 0, 0, 0],
            ]
        };

        let mut ppu = PPU::new();

        // Write the tile data to first tile in the PPU
        let mut address = TILESET_START_ADDRESS;
        for x in tile_data {
            ppu.bus_write8(address, x);
            address += 1;
        }
        // Write the tile data to the last tile in the PPU
        address = TILESET_END_ADDRESS - TILE_SIZE + 1;
        for x in tile_data {
            ppu.bus_write8(address, x);
            address += 1;
        }

        // Make sure the tile was written in the first and last locations.
        for y in 0..TILE_DIMENSION{
            for x in 0..TILE_DIMENSION{
                assert_eq!(expected_tile.read_pixel(x as u8, y as u8), ppu.tiles[0].read_pixel(x as u8, y as u8));
                assert_eq!(expected_tile.read_pixel(x as u8, y as u8), ppu.tiles[ppu.tiles.len()-1].read_pixel(x as u8, y as u8));
            }
        }

        // Make sure the raw tile data can be accessed correctly.
        address = TILESET_START_ADDRESS;
        for x in tile_data {
            println!("address: {}", address);
            assert_eq!(x, ppu.bus_read8(address));
            address += 1;
        }
        address = TILESET_END_ADDRESS - TILE_SIZE + 1;
        for x in tile_data {
            assert_eq!(x, ppu.bus_read8(address));
            address += 1;
        }
    }

    fn test_pack() -> (PPU, Ram, InterruptStatus){
        let mut ppu = PPU::new();
        let is = InterruptStatus::new();
        let ram = Ram::new(0x10000, 0);
        (ppu, ram, is)
    }

    impl PPU{
        fn run(&mut self, ticks:u16, bus: &mut impl BusRW, is: &mut InterruptStatus) {
            assert_eq!(ticks % 4, 0, "");
            for _ in 0..ticks/4 {
                self.execute_ticks(4, bus, is);
            }
        }
    }

    #[test]
    fn test_sprite_write() {
        let ref_sprite = OamSprite{
            ypos:1,
            xpos:2,
            tile:127,
            behind_background: true,
            yflip: false,
            xflip: true,
            palette: false
        };
        let ref_sprite_data = [1 as u8, 2, 127, 0xA0];

        // construct a ppu to test against
        let mut ppu = PPU::new();

        // Write the sprite data to the first and last sprite
        for (i, value) in ref_sprite_data.iter().enumerate() {
            ppu.bus_write8(OAM_START_ADDRESS + i, *value);
            ppu.bus_write8(OAM_END_ADDRESS - OAM_SPRITE_SIZE + i, *value);
        }

        assert_eq!(ref_sprite, ppu.sprites[0]);
        assert_eq!(ref_sprite, ppu.sprites[OAM_SPRITE_COUNT-1]);
        assert_eq!(ppu.sprite_data[0..OAM_SPRITE_SIZE], ref_sprite_data[..]);
        assert_eq!(ppu.sprite_data[(OAM_SPRITE_COUNT-1) * OAM_SPRITE_SIZE..], ref_sprite_data[..]);
    }

    #[test]
    fn test_lcdc_write() {
        let mut ppu = PPU::new();
        ppu.bus_write8(0xFF40, 0xAA);

        assert_eq!(ppu.bus_read8(LCDC_ADDRESS), 0xAA);
        assert_eq!(ppu.lcd_enabled, true);
        assert_eq!(ppu.window_tiles_high, false);
        assert_eq!(ppu.window_enabled, true);
        assert_eq!(ppu.bg_window_signed_addressing, false);
        assert_eq!(ppu.bg_tiles_high, true);
        assert_eq!(ppu.obj_double_sprites, false);
        assert_eq!(ppu.obj_enabled, true);
        assert_eq!(ppu.bg_window_enable, false)
    }

    #[test]
    fn test_scy_rw() {
        let mut ppu = PPU::new();
        let value = 39;
        let address = 0xFF42;
        ppu.bus_write8(address, value);
        assert_eq!(ppu.scroll_y, value);
        assert_eq!(ppu.bus_read8(address), value);
    }

    #[test]
    fn test_scx_rw() {
        let mut ppu = PPU::new();
        let value = 84;
        let address = 0xFF43;
        ppu.bus_write8(address, value);
        assert_eq!(ppu.scroll_x, value);
        assert_eq!(ppu.bus_read8(address), value);
    }

    #[test]
    fn test_ly_write_dead() {
        // The line y register is read only, and should not change due to a write.
        let mut ppu = PPU::new();
        let value = 84;
        let address = 0xFF44;
        ppu.bus_write8(address, value);
        assert_eq!(ppu.line_y, 0);
        assert_eq!(ppu.bus_read8(address), 0);
    }

    #[test]
    #[ignore]
    fn test_ly_read(){
        // TODO - this must return the current line once rendering functionality is in place.
        assert_eq!(1,2);
    }

    #[test]
    fn test_lyc_rw() {
        let mut ppu = PPU::new();
        let value = 255;
        let address = 0xFF45;
        ppu.bus_write8(address, value);
        assert_eq!(ppu.line_compare_value, value);
        assert_eq!(ppu.bus_read8(address), value);
    }

    #[test]
    fn test_wy_rw() {
        let mut ppu = PPU::new();
        let value = 43;
        let address = 0xFF4A;
        ppu.bus_write8(address, value);
        assert_eq!(ppu.window_y, value);
        assert_eq!(ppu.bus_read8(address), value);
    }

    #[test]
    fn test_wx_rw() {
        let mut ppu = PPU::new();
        let value = 43;
        let address = 0xFF4B;
        ppu.bus_write8(address, value);
        assert_eq!(ppu.window_x, value);
        assert_eq!(ppu.bus_read8(address), value);
    }

    #[test]
    fn test_lcds_write() {
        let mut ppu = PPU::new();
        let value = 0xFF;
        let address = 0xFF41;
        ppu.bus_write8(address, value);
        assert_eq!(ppu.bus_read8(address), 0x78);
        assert_eq!(ppu.line_compare_is, true);
        assert_eq!(ppu.mode2_is, true);
        assert_eq!(ppu.mode1_is, true);
        assert_eq!(ppu.mode0_is, true);

        ppu.bus_write8(address, 0b0010_1000);
        assert_eq!(ppu.mode0_is, true);
        assert_eq!(ppu.mode1_is, false);
        assert_eq!(ppu.mode2_is, true);
        assert_eq!(ppu.line_compare_is, false);
    }

    fn test_palette_rw(ppu:&mut PPU, palette:&Palette, address:usize){
    }

    #[test]
    fn test_bg_palette_rw(){
        let address = 0xFF47;
        let mut ppu = PPU::new();
        let raw_value = 0b_11_10_01_00;
        let expected_table = [0,1,2,3];

        // Code under test
        ppu.bus_write8(address, raw_value);

        assert_eq!(ppu.bus_read8(address), raw_value);
        assert_eq!(ppu.bg_palette.raw, raw_value);
        assert_eq!(ppu.bg_palette.table, expected_table);
    }

    #[test]
    fn test_obj_palette1_rw(){
        let address = 0xFF48;
        let mut ppu = PPU::new();
        let raw_value = 0b_11_10_01_00;
        let expected_table = [0,1,2,3];

        // Code under test
        ppu.bus_write8(address, raw_value);

        // Postconditions
        assert_eq!(ppu.bus_read8(address), raw_value);
        assert_eq!(ppu.obj_palette1.raw, raw_value);
        assert_eq!(ppu.obj_palette1.table, expected_table);
    }

    #[test]
    fn test_obj_palette2_rw(){
        let address = 0xFF49;
        let mut ppu = PPU::new();
        let raw_value = 0b_11_10_01_00;
        let expected_table = [0,1,2,3];

        // Code under test
        ppu.bus_write8(address, raw_value);

        // Postconditions
        assert_eq!(ppu.bus_read8(address), raw_value);
        assert_eq!(ppu.obj_palette2.raw, raw_value);
        assert_eq!(ppu.obj_palette2.table, expected_table);
    }

    #[test]
    fn test_dma_transfer_start_ticksAndAddr() {
        let mut ppu = PPU::new();
        let address = 0xFF46;
        let value = 45;
        let transfer_address = 45 * 0x100;

        ppu.bus_write8(address, 45);

        assert_eq!(ppu.bus_read8(address), value);
        assert_eq!(ppu.oam_dma_src, transfer_address);
        assert_eq!(ppu.oam_dma_ticks, 160);
    }

    #[test]
    fn test_dma_transfer_ticks_down() {
        // Given a PPU with a staged DMA transfer
        let (mut ppu, mut ram, mut is) = test_pack();

        // let mut ppu = PPU::new();
        // let mut ram = Ram::new(160, 0);
        ppu.bus_write8(OAM_DMA_REGISTER_ADDRESS, 0);

        // When 7 ticks are executed
        ppu.execute_ticks(7, &mut ram, &mut is);

        // Then the ticks count down by the number of executed ticks
        assert_eq!(ppu.oam_dma_ticks, 153);

        // When more ticks are executed than remain on the dma transfer
        ppu.execute_ticks(154, &mut ram, &mut is);

        // Then the ticks will not underflow.
        assert_eq!(ppu.oam_dma_ticks, 0);
    }

    #[test]
    fn test_dma_transfer_moves_data()
    {
        // Given a PPU with a staged DMA transfer, and some initialized ram
        let mut ppu = PPU::new();
        let mut ram = Ram::new(1024, 0);
        let mut is = InterruptStatus::new();
        for x in 256..(256+OAM_RAM_SIZE){
            ram.bus_write8(x, x as u8);
        }
        ppu.bus_write8(OAM_DMA_REGISTER_ADDRESS, 1);

        // When the ppu executes ticks
        ppu.execute_ticks(1, &mut ram, &mut is);

        // Then the OAM memory must contain the new data from the transfer source.
        for x in 0..OAM_RAM_SIZE {
            assert_eq!(ppu.sprite_data[x], x as u8);
        }
    }

    #[test]
    #[ignore]
    fn test_dma_memory_lock() {
        panic!("test_dma_memory_lock is not implemented");
    }

    #[test]
    fn test_tilemap_read_write() {
        let mut ppu = PPU::new();
        let tile_start = 0x9800;
        let tile_end = 0x9FFF;
        let start_value = 0xFC;
        let end_value = 0xFD;

        ppu.bus_write8(tile_start, start_value);
        ppu.bus_write8(tile_end, end_value);
        assert_eq!(ppu.bus_read8(tile_start), start_value);
        assert_eq!(ppu.bus_read8(tile_end), end_value);
        assert_eq!(ppu.tilemaps[0], start_value);
        assert_eq!(ppu.tilemaps[2047], end_value);
    }

    #[test]
    fn test_cycles_through_drawing_modes() {
        // Currently very flawed. Does not account for different timing within a line.
        let (mut ppu, mut ram, mut is) = test_pack();
        ppu.lcd_enabled = true;

        ppu.run(4, &mut ram, &mut is);
        assert_eq!(ppu.mode, Mode::SPRITE_SEARCH);
        ppu.run(72, &mut ram, &mut is);
        assert_eq!(ppu.mode, Mode::SPRITE_SEARCH);

        ppu.run(4, &mut ram, &mut is);
        assert_eq!(ppu.mode, Mode::LCD_TRANSFER);
        ppu.run(168, &mut ram, &mut is);
        assert_eq!(ppu.mode, Mode::LCD_TRANSFER);

        ppu.run(4, &mut ram, &mut is);
        assert_eq!(ppu.mode, Mode::HBLANK);
        ppu.run(200, &mut ram, &mut is);
        assert_eq!(ppu.mode, Mode::HBLANK);

        ppu.run(4, &mut ram, &mut is);
        assert_eq!(ppu.mode, Mode::SPRITE_SEARCH);
    }

    #[test]
    #[ignore]
    fn test_cycles_more_accurate_modes_for_real(){
        assert!(false);
    }

    #[test]
    fn test_ycomp_stat_interrupt() {
        let (mut ppu, mut ram, mut is) = test_pack();
        is.isrmask = 0xFF;
        ppu.lcd_enabled = true;
        ppu.line_compare_is = true;
        ppu.line_compare_value = 1;

        // Line zero, no interrupt.
        ppu.run(452, &mut ram, &mut is);
        assert_eq!(ppu.line_y, 0);
        assert_eq!(is.is_lcdstat_active(), false);

        // Line 1, interrupt on first tick.
        ppu.run(4, &mut ram, &mut is);
        assert_eq!(ppu.line_y, 1);
        assert_eq!(is.is_lcdstat_active(), true);

        // Line 1, No interrupt on subsequent tick.
        is.clear_lcdstat();
        ppu.run(4, &mut ram, &mut is);
        assert_eq!(is.is_lcdstat_active(), false);
        assert_eq!(ppu.line_y, 1);
    }

    #[test]
    fn test_hblank_stat_interrupt() {
        // This is not accurate, since it does not account for variable line timing.
        let (mut ppu, mut ram, mut is) = test_pack();
        is.isrmask = 0xFF;
        ppu.lcd_enabled = true;
        ppu.mode0_is = true;

        // No interrupt yet.
        ppu.run(80 + 172 - 4, &mut ram, &mut is);
        assert_eq!(is.is_lcdstat_active(), false);

        // Transition in to HBLANK, interrupt asserts
        ppu.run(4, &mut ram, &mut is);
        assert_eq!(is.is_lcdstat_active(), true);
        assert_eq!(ppu.mode, Mode::HBLANK);

        // Continue in HBLANK, interrupt is not asserted anymore.
        is.clear_lcdstat();
        ppu.run(4, &mut ram, &mut is);
        assert_eq!(is.is_lcdstat_active(), false);
    }

    #[test]
    fn test_vblank_interrupts() {
        let (mut ppu, mut ram, mut is) = test_pack();

        is.isrmask = 0xFF;
        ppu.lcd_enabled = true;
        ppu.mode1_is = true;

        // Get right to the edge of vblank
        for _ in 0..143 {
            ppu.run(456, &mut ram, &mut is);
        }
        assert_eq!(ppu.line_y, 143);
        assert_eq!(is.is_vblank_active(), false);
        assert_eq!(is.is_lcdstat_active(), false);

        // Draw the line to trigger vblank.
        ppu.run(456, &mut ram, &mut is);
        assert_eq!(is.is_vblank_active(), true);
        assert_eq!(is.is_lcdstat_active(), true);

        // Draw another line. No more interrupts should activate.
        is.clear_vblank();
        is.clear_lcdstat();
        ppu.run(456, &mut ram, &mut is);
        assert_eq!(is.is_vblank_active(), false);
        assert_eq!(is.is_lcdstat_active(), false);
    }
}