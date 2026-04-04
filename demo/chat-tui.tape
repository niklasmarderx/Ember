# Ember Chat + TUI — Part 2
# Run: vhs demo/chat-tui.tape

Output demo/chat-tui.gif

Require ember

Set Shell "bash"
Set FontSize 15
Set Width 1000
Set Height 650
Set Padding 16
Set Theme "Catppuccin Mocha"
Set TypingSpeed 30ms
Set PlaybackSpeed 2.5
Set CursorBlink false
Set Framerate 20

# Setup
Hide
Type `export PS1="\033[1;33m$\033[0m "`
Enter
Sleep 200ms
Type "clear"
Enter
Sleep 500ms
Show

# ━━━━ Chat mode ━━━━
Type "ember chat"
Sleep 300ms
Enter

# Wait for chat prompt
Sleep 4s

Type "Read Cargo.toml and tell me how many crates this workspace has"
Sleep 300ms
Enter

# Wait for tool use + response
Sleep 18s

# Exit chat
Ctrl+C
Sleep 500ms
Ctrl+C
Sleep 2s

# ━━━━ TUI mode ━━━━
Type "ember tui"
Sleep 300ms
Enter
Sleep 4s

Type "Create hello.rs with a colorful Rust greeting using ANSI codes, then compile and run it"
Sleep 300ms
Enter

# Wait for code generation + execution
Sleep 25s

# Exit TUI
Ctrl+C
Sleep 2s

# Show result
Type "cat hello.rs"
Sleep 200ms
Enter
Sleep 3s

# Cleanup
Hide
Type "rm -f hello.rs hello"
Enter
Show
Sleep 1s
