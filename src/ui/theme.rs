use ratatui::style::Color;

// Primary brand colors
pub const ACCENT: Color = Color::Rgb(218, 118, 89);        // #DA7659 - warm orange
pub const ACCENT_DIM: Color = Color::Rgb(178, 98, 69);     // Dimmed warm orange
pub const SUCCESS: Color = Color::Rgb(134, 188, 111);      // Soft green
pub const WARNING: Color = Color::Rgb(229, 192, 123);      // Warm amber

// Text colors
pub const TEXT: Color = Color::Rgb(240, 240, 240);         // #f0f0f0 - primary text
pub const TEXT_SECONDARY: Color = Color::Rgb(180, 180, 180); // Secondary text
pub const TEXT_MUTED: Color = Color::Rgb(144, 144, 144);   // #909090 - muted text

// Background colors
pub const BG_BASE: Color = Color::Rgb(34, 34, 32);         // #222220 - darkest background
pub const BG_SURFACE: Color = Color::Rgb(51, 51, 49);      // #333331 - content panels
pub const BG_ELEVATED: Color = Color::Rgb(66, 66, 64);     // Elevated elements
pub const BG_INPUT: Color = Color::Rgb(58, 58, 56);        // #3a3a38 - input field

// Border colors
pub const BORDER: Color = Color::Rgb(66, 66, 64);          // Subtle border
pub const BORDER_FOCUS: Color = Color::Rgb(218, 118, 89);  // Accent color for focus

// Role-specific colors
pub const USER: Color = Color::Rgb(218, 118, 89);          // Warm orange for user
pub const ASSISTANT: Color = Color::Rgb(144, 144, 144);    // Muted for assistant
