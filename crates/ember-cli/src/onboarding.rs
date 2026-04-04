//! First-run onboarding for Ember -- with a hatching coding buddy!
//!
//! Features:
//! - Interactive arrow-key navigation (crossterm raw mode)
//! - ASCII egg-hatching animation with a random coding buddy
//! - Random buddy attributes (name, personality, specialty, symbol)
//! - Typewriter text effects
//! - Profile saved to `~/.ember/profile.toml`

use anyhow::Result;
use colored::Colorize;
use crossterm::{
    cursor,
    event::{self, Event, KeyCode, KeyEvent, KeyModifiers},
    execute,
    terminal::{self, ClearType},
};
use serde::{Deserialize, Serialize};
use std::io::{self, Write};
use std::path::PathBuf;
use std::thread;
use std::time::Duration;

// ──────────────────────────────────────────────────────────────────────────────
// Coding Buddy
// ──────────────────────────────────────────────────────────────────────────────

/// The buddy species with their ASCII art.
/// Tuple: (species_name, ascii_symbol, ascii_art_lines)
const BUDDY_SPECIES: &[(&str, &str, &[&str])] = &[
    (
        "Phoenix",
        "[P]",
        &[
            r"    ,///,",
            r"   {  o o }",
            r"    \  ^ /",
            r"    /`---'\",
            r"   / /| |\ \",
            r"  (_/ | | \_)",
            r"      | |",
            r"     _|_|_",
        ],
    ),
    (
        "Dragon",
        "[D]",
        &[
            r"       __====-_  _-====__",
            r"    _--^^^#####//      \\#####^^^--_",
            r"  _-^##########// (    ) \\##########^-_",
            r" -############//  |\^^/|  \\############-",
            r"   _/########//   (@::@)   \\########\_",
            r"  /#######//     \\  //     \\#######\",
            r"  -###########\\    (oo)    //###########-",
            r"   -############\\  / VV \  //############-",
        ],
    ),
    (
        "Owl",
        "[O]",
        &[
            r"   {o,o}",
            r"   |)__)",
            r"   -'--'-",
        ],
    ),
    (
        "Cat",
        "[C]",
        &[
            r"    /\_/\  ",
            r"   ( o.o ) ",
            r"    > ^ <  ",
            r"   /|   |\ ",
            r"  (_|   |_)",
        ],
    ),
    (
        "Robot",
        "[R]",
        &[
            r"   +-----+  ",
            r"   | o  o |  ",
            r"   | \_-/ |  ",
            r"   +-+-+-++  ",
            r"   +-+-+-+-+ ",
            r"   | | | | | ",
            r"   +-------+ ",
        ],
    ),
    (
        "Fox",
        "[F]",
        &[
            r"    /\   /\ ",
            r"   /  \ /  \",
            r"  | .  _  . |",
            r"   \  w  / ",
            r"    '---'  ",
        ],
    ),
    (
        "Octopus",
        "[8]",
        &[
            r"     ___   ",
            r"    (o o)  ",
            r"   /( _ )\ ",
            r"  / /| |\ \",
            r"  \/ | | \/",
            r"     ~ ~   ",
        ],
    ),
    (
        "Penguin",
        "[>]",
        &[
            r"     .--.  ",
            r"    |o_o | ",
            r"    |:_/ | ",
            r"   //   \ \",
            r"  (|     | )",
            r" /'\_   _/`\",
            r" \___)=(___/",
        ],
    ),
];

const BUDDY_PERSONALITIES: &[&str] = &[
    "enthusiastic",
    "calm & focused",
    "witty & sarcastic",
    "encouraging",
    "meticulous",
    "adventurous",
    "philosophical",
    "playful",
];

const BUDDY_SPECIALTIES: &[&str] = &[
    "debugging nightmares",
    "clean architecture",
    "performance tuning",
    "writing tests",
    "refactoring legacy code",
    "API design",
    "system design",
    "code review",
    "documentation",
    "rapid prototyping",
];

const BUDDY_TITLES: &[&str] = &[
    "Code Wizard",
    "Bug Slayer",
    "Syntax Sorcerer",
    "Refactor Ninja",
    "Stack Whisperer",
    "Memory Guardian",
    "Type Sage",
    "Build Master",
    "Test Champion",
    "Deploy Captain",
];

/// A random coding buddy that hatches from the egg.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodingBuddy {
    pub species: String,
    pub emoji: String,
    pub name: String,
    pub personality: String,
    pub specialty: String,
    pub title: String,
    pub level: u32,
    pub xp: u32,
}

impl CodingBuddy {
    /// Generate a random buddy using a simple hash-based RNG.
    pub fn hatch() -> Self {
        let seed = {
            use std::time::SystemTime;
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap_or_default()
                .subsec_nanos() as usize
                ^ std::process::id() as usize
        };

        let species_idx = seed % BUDDY_SPECIES.len();
        let personality_idx = (seed / 7) % BUDDY_PERSONALITIES.len();
        let specialty_idx = (seed / 13) % BUDDY_SPECIALTIES.len();
        let title_idx = (seed / 17) % BUDDY_TITLES.len();

        let (species, symbol, _art) = BUDDY_SPECIES[species_idx];

        Self {
            species: species.to_string(),
            emoji: symbol.to_string(),
            name: format!("{}", species),
            personality: BUDDY_PERSONALITIES[personality_idx].to_string(),
            specialty: BUDDY_SPECIALTIES[specialty_idx].to_string(),
            title: BUDDY_TITLES[title_idx].to_string(),
            level: 1,
            xp: 0,
        }
    }

    /// Get the ASCII art for this buddy.
    pub fn ascii_art(&self) -> Vec<&'static str> {
        for (species, _symbol, art) in BUDDY_SPECIES {
            if *species == self.species {
                return art.to_vec();
            }
        }
        vec!["  ???  "]
    }

    /// Format buddy stats as a card.
    pub fn format_card(&self) -> String {
        let art = self.ascii_art();
        let mut lines = Vec::new();
        lines.push(format!(
            "   {} {} the {} -- {}",
            self.emoji.bright_cyan(),
            self.name.bright_yellow().bold(),
            self.title.bright_cyan(),
            format!("Lv.{}", self.level).bright_green()
        ));
        lines.push(format!(
            "   Personality: {}  |  Specialty: {}",
            self.personality.bright_magenta(),
            self.specialty.bright_blue()
        ));
        lines.push(String::new());
        for line in &art {
            lines.push(format!("   {}", line.bright_yellow()));
        }
        lines.join("\n")
    }

    /// XP required to reach the next level.
    pub fn xp_for_next_level(&self) -> u32 {
        self.level * 100
    }

    /// Award XP to the buddy and return a level-up message if leveled up.
    pub fn award_xp(&mut self, amount: u32) -> Option<String> {
        self.xp += amount;
        let needed = self.xp_for_next_level();
        if self.xp >= needed {
            self.xp -= needed;
            self.level += 1;
            // Pick a new title at certain milestones
            let new_title = match self.level {
                2 => "Apprentice Coder",
                3 => "Code Adept",
                5 => "Senior Debugger",
                7 => "Architecture Sage",
                10 => "Legendary Hacker",
                15 => "Code Demigod",
                20 => "Transcendent Engineer",
                _ => &self.title,
            };
            let old_title = self.title.clone();
            if new_title != old_title {
                self.title = new_title.to_string();
                Some(format!(
                    "{} {} leveled up to Lv.{}! New title: {} -> {}",
                    self.emoji, self.name, self.level, old_title, self.title
                ))
            } else {
                Some(format!(
                    "{} {} leveled up to Lv.{}!",
                    self.emoji, self.name, self.level
                ))
            }
        } else {
            None
        }
    }

    /// Format XP progress bar.
    pub fn xp_bar(&self) -> String {
        let needed = self.xp_for_next_level();
        let filled = if needed > 0 {
            (self.xp as f32 / needed as f32 * 20.0) as usize
        } else {
            0
        };
        let empty = 20 - filled.min(20);
        format!(
            "[{}{}] {}/{} XP",
            "#".repeat(filled),
            "-".repeat(empty),
            self.xp,
            needed
        )
    }

    /// System prompt addition describing the buddy's personality.
    pub fn to_system_context(&self) -> String {
        format!(
            "## Your Persona\n\
             You are {} the {}, a coding buddy with a {} personality.\n\
             Your specialty is {}.\n\
             Be true to your personality in your responses.\n\
             IMPORTANT: Never use emojis in your responses. Use plain text and ASCII only.",
            self.name, self.title, self.personality, self.specialty
        )
    }
}

/// Award XP to the user's buddy after a chat session and save the profile.
/// Returns a level-up message if the buddy leveled up.
pub fn award_session_xp(turns: usize) -> Option<String> {
    let mut profile = load_profile()?;
    let buddy = profile.buddy.as_mut()?;
    // Award 10 XP per turn, minimum 5 XP per session
    let xp = (turns as u32 * 10).max(5);
    let msg = buddy.award_xp(xp);
    let _ = save_profile(&profile);
    msg
}

// ──────────────────────────────────────────────────────────────────────────────
// User Profile
// ──────────────────────────────────────────────────────────────────────────────

/// User profile saved to `~/.ember/profile.toml`.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UserProfile {
    pub name: String,
    pub language: String,
    pub role: String,
    pub experience: String,
    pub tech_stack: String,
    pub style: String,
    pub buddy: Option<CodingBuddy>,
}

impl UserProfile {
    pub fn to_system_context(&self) -> String {
        let mut lines = Vec::new();
        lines.push("## User Profile".to_string());
        if !self.name.is_empty() {
            lines.push(format!("- Name: {}", self.name));
        }
        if !self.language.is_empty() {
            lines.push(format!("- Preferred language: {}", self.language));
        }
        if !self.role.is_empty() {
            lines.push(format!("- Role: {}", self.role));
        }
        if !self.experience.is_empty() {
            lines.push(format!("- Experience level: {}", self.experience));
        }
        if !self.tech_stack.is_empty() {
            lines.push(format!("- Tech stack: {}", self.tech_stack));
        }
        if !self.style.is_empty() {
            lines.push(format!(
                "- Communication style: {} (adapt your responses accordingly)",
                self.style
            ));
        }
        if let Some(ref buddy) = self.buddy {
            lines.push(String::new());
            lines.push(buddy.to_system_context());
        }
        lines.join("\n")
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// Terminal helpers
// ──────────────────────────────────────────────────────────────────────────────

/// Print text with a typewriter effect.
fn typewriter(text: &str, delay_ms: u64) {
    for ch in text.chars() {
        print!("{}", ch);
        io::stdout().flush().ok();
        if delay_ms > 0 {
            thread::sleep(Duration::from_millis(delay_ms));
        }
    }
}

/// Print text with a typewriter effect and newline.
fn typewriter_ln(text: &str, delay_ms: u64) {
    typewriter(text, delay_ms);
    println!();
}

/// Sleep for ms.
fn pause(ms: u64) {
    thread::sleep(Duration::from_millis(ms));
}

/// Clear the current line.
fn clear_line() {
    execute!(io::stdout(), cursor::MoveToColumn(0), terminal::Clear(ClearType::CurrentLine)).ok();
}

// ──────────────────────────────────────────────────────────────────────────────
// Interactive menu (arrow key navigation)
// ──────────────────────────────────────────────────────────────────────────────

/// Display an interactive menu and return the selected index.
/// Supports arrow keys (Up/Down), Enter to confirm, number keys.
fn interactive_menu(prompt: &str, options: &[&str], default_idx: usize) -> usize {
    let mut selected = default_idx;

    // Switch to raw mode for key capture
    terminal::enable_raw_mode().ok();

    // Print the prompt
    print!("\r\n   {}\r\n", prompt.bright_cyan().bold());
    io::stdout().flush().ok();

    // Draw initial menu
    draw_menu(options, selected);

    loop {
        if let Ok(Event::Key(KeyEvent { code, modifiers, .. })) = event::read() {
            // Handle Ctrl+C
            if modifiers.contains(KeyModifiers::CONTROL) && code == KeyCode::Char('c') {
                terminal::disable_raw_mode().ok();
                println!();
                std::process::exit(0);
            }

            match code {
                KeyCode::Up | KeyCode::Char('k') => {
                    if selected > 0 {
                        selected -= 1;
                    } else {
                        selected = options.len() - 1;
                    }
                    redraw_menu(options, selected);
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    if selected < options.len() - 1 {
                        selected += 1;
                    } else {
                        selected = 0;
                    }
                    redraw_menu(options, selected);
                }
                KeyCode::Enter => {
                    break;
                }
                KeyCode::Char(c) if c.is_ascii_digit() => {
                    let n = c as usize - '0' as usize;
                    if n >= 1 && n <= options.len() {
                        selected = n - 1;
                        redraw_menu(options, selected);
                        pause(150);
                        break;
                    }
                }
                _ => {}
            }
        }
    }

    terminal::disable_raw_mode().ok();

    // Clear menu and print selection
    let lines_to_clear = options.len();
    for _ in 0..lines_to_clear {
        execute!(io::stdout(), cursor::MoveUp(1), terminal::Clear(ClearType::CurrentLine)).ok();
    }

    // Reprint selected option nicely
    print!(
        "\r   {} {}\r\n",
        ">".bright_green(),
        options[selected].bright_white().bold()
    );
    io::stdout().flush().ok();

    selected
}

/// Draw the menu options.
fn draw_menu(options: &[&str], selected: usize) {
    let stdout = io::stdout();
    let mut out = stdout.lock();
    for (i, opt) in options.iter().enumerate() {
        if i == selected {
            write!(
                out,
                "\r     {} {} {}\r\n",
                ">".bright_green().bold(),
                format!("{}", i + 1).bright_green(),
                opt.bright_white().bold()
            )
            .ok();
        } else {
            write!(
                out,
                "\r       {} {}\r\n",
                format!("{}", i + 1).dimmed(),
                opt.dimmed()
            )
            .ok();
        }
    }
    out.flush().ok();
}

/// Redraw menu by moving cursor up and overwriting.
fn redraw_menu(options: &[&str], selected: usize) {
    let stdout = io::stdout();
    let mut out = stdout.lock();
    for _ in 0..options.len() {
        execute!(out, cursor::MoveUp(1)).ok();
    }
    for (i, opt) in options.iter().enumerate() {
        if i == selected {
            write!(
                out,
                "\r     {} {} {}\r\n",
                ">".bright_green().bold(),
                format!("{}", i + 1).bright_green(),
                opt.bright_white().bold()
            )
            .ok();
        } else {
            write!(
                out,
                "\r       {} {}\r\n",
                format!("{}", i + 1).dimmed(),
                opt.dimmed()
            )
            .ok();
        }
    }
    out.flush().ok();
}

/// Read a line of text in raw mode with proper echo.
fn raw_input(prompt: &str, default: &str) -> String {
    terminal::enable_raw_mode().ok();

    let default_hint = if default.is_empty() {
        String::new()
    } else {
        format!(" {}", format!("[{}]", default).dimmed())
    };

    print!("\r\n   {}{} ", prompt.bright_cyan(), default_hint);
    io::stdout().flush().ok();

    let mut buf = String::new();

    loop {
        if let Ok(Event::Key(KeyEvent { code, modifiers, .. })) = event::read() {
            if modifiers.contains(KeyModifiers::CONTROL) && code == KeyCode::Char('c') {
                terminal::disable_raw_mode().ok();
                println!();
                std::process::exit(0);
            }

            match code {
                KeyCode::Enter => {
                    print!("\r\n");
                    io::stdout().flush().ok();
                    break;
                }
                KeyCode::Backspace => {
                    if !buf.is_empty() {
                        buf.pop();
                        print!("\x08 \x08");
                        io::stdout().flush().ok();
                    }
                }
                KeyCode::Char(c) => {
                    buf.push(c);
                    print!("{}", c);
                    io::stdout().flush().ok();
                }
                _ => {}
            }
        }
    }

    terminal::disable_raw_mode().ok();

    let trimmed = buf.trim().to_string();
    if trimmed.is_empty() {
        default.to_string()
    } else {
        trimmed
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// Egg hatching animation
// ──────────────────────────────────────────────────────────────────────────────

fn egg_animation(buddy: &CodingBuddy) {
    // Height of each frame (must be consistent for smooth redraw)
    const FRAME_HEIGHT: usize = 9;

    let frames: Vec<(&[&str], &str, u64)> = vec![
        // (ascii_lines, sound_effect, pause_ms)
        // Phase 1: The egg sits quietly
        (&[
            "              ",
            "      ____    ",
            "     /    \\   ",
            "    /      \\  ",
            "   |        | ",
            "   |        | ",
            "    \\      /  ",
            "     \\____/   ",
            "              ",
        ], "", 900),
        // Phase 2: Slight wobble left
        (&[
            "              ",
            "     ____     ",
            "    /    \\    ",
            "   /      \\   ",
            "  |        |  ",
            "  |        |  ",
            "   \\      /   ",
            "    \\____/    ",
            "              ",
        ], "  ...?", 350),
        // Phase 3: Wobble right
        (&[
            "              ",
            "       ____   ",
            "      /    \\  ",
            "     /      \\ ",
            "    |        |",
            "    |        |",
            "     \\      / ",
            "      \\____/  ",
            "              ",
        ], "", 350),
        // Phase 4: Back to center + first hairline crack
        (&[
            "              ",
            "      ____    ",
            "     /  . \\   ",
            "    /   |  \\  ",
            "   |    |   | ",
            "   |        | ",
            "    \\      /  ",
            "     \\____/   ",
            "              ",
        ], "  *tick*", 500),
        // Phase 5: Wobble harder left
        (&[
            "              ",
            "    ____      ",
            "   /  . \\     ",
            "  /  /|  \\    ",
            " |  / |   |   ",
            " |    |   |   ",
            "  \\      /    ",
            "   \\____/     ",
            "              ",
        ], "", 250),
        // Phase 6: Wobble harder right
        (&[
            "              ",
            "        ____  ",
            "       / .  \\ ",
            "      / /|  \\",
            "     |  /|   |",
            "     | / |   |",
            "      \\__|  / ",
            "       \\___/  ",
            "              ",
        ], "  *crack!*", 400),
        // Phase 7: Center, big cracks
        (&[
            "              ",
            "      _**_    ",
            "     / /\\ \\   ",
            "    / / |\\ \\  ",
            "   | /  | \\ | ",
            "   |/ \\ |  \\| ",
            "    \\ \\_|  /  ",
            "     \\__*_/   ",
            "              ",
        ], "  *CRACK!*", 500),
        // Phase 8: Shell splits apart
        (&[
            "              ",
            "    \\  _**  / ",
            "     ||/  ||  ",
            "    / |    \\  ",
            "   |  o  o  | ",
            "   |   __   | ",
            "    \\_/  \\_/  ",
            "   --/    \\-- ",
            "              ",
        ], "  *CRAAACK!!*", 400),
        // Phase 9: Explosion burst
        (&[
            "    \\  |  /   ",
            "  --- ___ --- ",
            "    / \\ / \\   ",
            "   |  o  o |  ",
            "   |  (__) |  ",
            "    \\      /  ",
            "  -- \\____/ --",
            "    / |  | \\  ",
            "   /  |  |  \\ ",
        ], "  !!!", 350),
        // Phase 10: Smoke clearing
        (&[
            "   .  .  .  . ",
            "  .  .    .   ",
            "   .   .   .  ",
            "   |  o  o |  ",
            "   |  (__) |  ",
            "    \\      /  ",
            "     \\____/   ",
            "              ",
            "              ",
        ], "", 400),
    ];

    println!();
    // Print initial empty lines so we can overwrite
    for _ in 0..FRAME_HEIGHT {
        println!();
    }

    for (i, (frame_lines, sound, delay)) in frames.iter().enumerate() {
        // Move cursor up to overwrite previous frame
        for _ in 0..FRAME_HEIGHT {
            execute!(
                io::stdout(),
                cursor::MoveUp(1),
                terminal::Clear(ClearType::CurrentLine)
            )
            .ok();
        }

        // Color the frame based on phase
        let color_fn: fn(&str) -> String = match i {
            0..=1 => |s: &str| s.white().to_string(),
            2..=3 => |s: &str| s.bright_white().to_string(),
            4..=5 => |s: &str| s.bright_yellow().to_string(),
            6 => |s: &str| s.bright_yellow().bold().to_string(),
            7 => |s: &str| s.bright_red().to_string(),
            8 => |s: &str| s.bright_red().bold().to_string(),
            9 => |s: &str| s.bright_cyan().to_string(),
            _ => |s: &str| s.white().to_string(),
        };

        for line in *frame_lines {
            println!("   {}", color_fn(line));
        }
        io::stdout().flush().ok();

        // Print sound effect on the side
        if !sound.is_empty() {
            let styled = match i {
                4..=5 => sound.bright_yellow().to_string(),
                6..=7 => sound.bright_red().bold().to_string(),
                8 => sound.bright_red().bold().to_string(),
                _ => sound.dimmed().to_string(),
            };
            print!("   {}", styled);
            io::stdout().flush().ok();
            pause(200);
            clear_line();
        }

        pause(*delay);
    }

    // Clear the last frame
    for _ in 0..FRAME_HEIGHT {
        execute!(
            io::stdout(),
            cursor::MoveUp(1),
            terminal::Clear(ClearType::CurrentLine)
        )
        .ok();
    }

    // === Show the buddy with a dramatic reveal ===
    println!();

    // Sparkle border top
    let sparkle_top = "- * - * - * - * - * -";
    println!("   {}", sparkle_top.bright_yellow().bold());
    println!();

    // Buddy ASCII art with staggered reveal
    let art = buddy.ascii_art();
    for line in &art {
        println!("   {}", line.bright_yellow().bold());
        io::stdout().flush().ok();
        pause(80);
    }

    println!();
    println!("   {}", sparkle_top.bright_yellow().bold());
    println!();

    pause(200);

    // Typewriter the buddy intro
    typewriter("   ", 0);
    typewriter(
        &format!(
            "A {} {} hatched!",
            buddy.personality.bright_magenta(),
            buddy.species.bright_yellow().bold()
        ),
        30,
    );
    println!();
    pause(400);

    typewriter("   ", 0);
    typewriter(
        &format!(
            "Meet {} {} the {}",
            buddy.emoji.bright_cyan(),
            buddy.name.bright_yellow().bold(),
            buddy.title.bright_cyan(),
        ),
        30,
    );
    println!();
    pause(300);

    typewriter("   ", 0);
    typewriter(
        &format!(
            "Specialty: {}",
            buddy.specialty.bright_blue()
        ),
        25,
    );
    println!();
    println!();
}

// ──────────────────────────────────────────────────────────────────────────────
// File I/O
// ──────────────────────────────────────────────────────────────────────────────

fn profile_path() -> Option<PathBuf> {
    dirs::home_dir().map(|h| h.join(".ember").join("profile.toml"))
}

pub fn load_profile() -> Option<UserProfile> {
    let path = profile_path()?;
    if !path.exists() {
        return None;
    }
    let content = std::fs::read_to_string(&path).ok()?;
    toml::from_str(&content).ok()
}

fn save_profile(profile: &UserProfile) -> Result<()> {
    let path = profile_path().ok_or_else(|| anyhow::anyhow!("Cannot determine home directory"))?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let content = toml::to_string_pretty(profile)?;
    std::fs::write(&path, content)?;
    Ok(())
}

// ──────────────────────────────────────────────────────────────────────────────
// Main onboarding flow
// ──────────────────────────────────────────────────────────────────────────────

pub fn run_onboarding() -> Result<UserProfile> {
    println!();
    println!();

    // Animated banner
    let banner_lines = [
        "   +====================================================+",
        "   |                                                      |",
        "   |         W E L C O M E   T O   E M B E R             |",
        "   |                                                      |",
        "   |           Your AI Coding Companion                   |",
        "   |                                                      |",
        "   +====================================================+",
    ];

    for (i, line) in banner_lines.iter().enumerate() {
        let colored = if i == 0 || i == banner_lines.len() - 1 {
            line.bright_yellow().bold().to_string()
        } else if i == 2 {
            line.bright_red().bold().to_string()
        } else {
            line.bright_yellow().to_string()
        };
        println!("{}", colored);
        pause(80);
    }

    println!();
    pause(300);

    typewriter_ln(
        &format!(
            "   {}",
            "Let's set up your profile and hatch your coding buddy!".bright_white()
        ),
        20,
    );
    typewriter_ln(
        &format!(
            "   {}",
            "Use arrow keys to navigate, Enter to select.".dimmed()
        ),
        15,
    );

    // ── Questions ──

    let name = raw_input("What's your name?", "");

    let lang_options = &["Deutsch", "English", "Espanol", "Francais", "Japanese", "Chinese", "Other"];
    let lang_idx = interactive_menu("Preferred language?", lang_options, 1);
    let language = lang_options[lang_idx].to_string();

    let role = raw_input(
        "Your role?",
        "developer",
    );

    let exp_options = &["Beginner", "Intermediate", "Expert", "Wizard"];
    let exp_idx = interactive_menu("Experience level?", exp_options, 1);
    let experience = match exp_idx {
        0 => "beginner",
        1 => "intermediate",
        2 => "expert",
        3 => "wizard",
        _ => "intermediate",
    }
    .to_string();

    let tech_stack = raw_input(
        "Favorite tech?",
        "",
    );

    let style_options = &[
        "Concise -- straight to the point",
        "Balanced -- moderate detail",
        "Detailed -- thorough explanations",
    ];
    let style_idx = interactive_menu("Communication style?", style_options, 0);
    let style = match style_idx {
        0 => "concise",
        1 => "balanced",
        2 => "detailed",
        _ => "concise",
    }
    .to_string();

    // ── Egg hatching! ──

    println!();
    typewriter_ln(
        &format!(
            "   {}",
            "Now let's hatch your coding buddy...".bright_yellow()
        ),
        25,
    );
    pause(500);

    let mut buddy = CodingBuddy::hatch();
    egg_animation(&buddy);

    // ── Name your buddy ──
    let custom_name = raw_input(
        &format!("Give {} a name? (Enter to keep '{}')", buddy.emoji, buddy.name),
        &buddy.name,
    );
    if !custom_name.is_empty() {
        buddy.name = custom_name;
    }

    // ── Personality tuning ──
    let personality_options = &[
        "Enthusiastic -- hype and energy!",
        "Calm & Focused -- zen coding",
        "Witty & Sarcastic -- roast my code",
        "Encouraging -- always positive",
        "Meticulous -- detail-obsessed",
        "Playful -- make it fun",
        "Philosophical -- deep thoughts",
        "No-nonsense -- just the code",
    ];
    let pers_idx = interactive_menu(
        &format!("Choose {}'s personality:", buddy.name),
        personality_options,
        0,
    );
    buddy.personality = match pers_idx {
        0 => "enthusiastic",
        1 => "calm & focused",
        2 => "witty & sarcastic",
        3 => "encouraging",
        4 => "meticulous",
        5 => "playful",
        6 => "philosophical",
        7 => "no-nonsense",
        _ => "enthusiastic",
    }.to_string();

    println!();
    println!(
        "   {} {} the {} -- {} is ready!",
        buddy.emoji.bright_cyan(),
        buddy.name.bright_yellow().bold(),
        buddy.personality.bright_magenta(),
        buddy.title.bright_cyan(),
    );

    // ── Save profile ──

    let profile = UserProfile {
        name,
        language,
        role,
        experience,
        tech_stack,
        style,
        buddy: Some(buddy.clone()),
    };

    save_profile(&profile)?;

    pause(300);

    // Success message
    println!(
        "   {} {}",
        "+".bright_green().bold(),
        "Profile saved!".bright_green()
    );
    println!(
        "   {} Update anytime with {}",
        "i".bright_blue(),
        "/profile".bright_cyan()
    );
    println!(
        "   {} {} is ready to code with you!",
        buddy.emoji.bright_cyan(),
        buddy.name.bright_yellow().bold(),
    );
    println!();

    // Divider before chat starts
    let divider = "=".repeat(50);
    println!("   {}", divider.dimmed());
    println!();

    Ok(profile)
}

/// Check if onboarding is needed, run it if so, and return the profile.
pub fn ensure_profile() -> Option<UserProfile> {
    if let Some(profile) = load_profile() {
        return Some(profile);
    }
    match run_onboarding() {
        Ok(profile) => Some(profile),
        Err(e) => {
            eprintln!(
                "{} Onboarding error: {}",
                "[ember]".bright_red(),
                e
            );
            None
        }
    }
}

/// Format a welcome-back message with the buddy.
pub fn welcome_back(profile: &UserProfile) -> String {
    if let Some(ref buddy) = profile.buddy {
        let greeting = if !profile.name.is_empty() {
            format!("Welcome back, {}!", profile.name)
        } else {
            "Welcome back!".to_string()
        };
        format!(
            "{} {} -- {} says hello!",
            buddy.emoji.to_string(),
            greeting.bright_yellow(),
            buddy.name.bright_cyan()
        )
    } else if !profile.name.is_empty() {
        format!("Welcome back, {}!", profile.name.bright_yellow())
    } else {
        "Welcome back!".to_string()
    }
}