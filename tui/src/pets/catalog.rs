//! Built-in pet catalog ported from the Codex App avatar catalog.

pub(super) const DEFAULT_FRAME_WIDTH: u32 = 192;
pub(super) const DEFAULT_FRAME_HEIGHT: u32 = 208;
pub(super) const DEFAULT_FRAME_COLUMNS: u32 = 8;
pub(super) const DEFAULT_FRAME_ROWS: u32 = 9;
pub(super) const SPRITESHEET_WIDTH: u32 = DEFAULT_FRAME_WIDTH * DEFAULT_FRAME_COLUMNS;
pub(super) const SPRITESHEET_HEIGHT: u32 = DEFAULT_FRAME_HEIGHT * DEFAULT_FRAME_ROWS;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct BuiltinPet {
    pub(super) id: &'static str,
    pub(super) spritesheet_file: &'static str,
}

pub(super) const BUILTIN_PETS: &[BuiltinPet] = &[
    BuiltinPet {
        id: "codex",
        spritesheet_file: "codex-spritesheet-v4.webp",
    },
    BuiltinPet {
        id: "dewey",
        spritesheet_file: "dewey-spritesheet-v4.webp",
    },
    BuiltinPet {
        id: "fireball",
        spritesheet_file: "fireball-spritesheet-v4.webp",
    },
    BuiltinPet {
        id: "rocky",
        spritesheet_file: "rocky-spritesheet-v4.webp",
    },
    BuiltinPet {
        id: "seedy",
        spritesheet_file: "seedy-spritesheet-v4.webp",
    },
    BuiltinPet {
        id: "stacky",
        spritesheet_file: "stacky-spritesheet-v4.webp",
    },
    BuiltinPet {
        id: "bsod",
        spritesheet_file: "bsod-spritesheet-v4.webp",
    },
    BuiltinPet {
        id: "null-signal",
        spritesheet_file: "null-signal-spritesheet-v4.webp",
    },
];

pub(super) fn builtin_pet(id: &str) -> Option<BuiltinPet> {
    BUILTIN_PETS.iter().copied().find(|pet| pet.id == id)
}

#[cfg(test)]
pub(super) fn write_test_spritesheet(path: &std::path::Path) {
    let image = image::RgbaImage::new(SPRITESHEET_WIDTH, SPRITESHEET_HEIGHT);
    image.save(path).unwrap();
}
