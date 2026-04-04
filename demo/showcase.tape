# Ember Complete Showcase — Onboarding + Chat exit + TUI
# Run: vhs demo/showcase.tape

Output demo/showcase.gif

Require ember

Set Shell "bash"
Set FontSize 15
Set Width 1000
Set Height 650
Set Padding 16
Set Theme "Catppuccin Mocha"
Set TypingSpeed 30ms
Set PlaybackSpeed 1.5
Set CursorBlink false
Set Framerate 10

# ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
# Setup: hide username, clean profile
# ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
Hide
Type `export PS1="\033[1;33m$\033[0m "`
Enter
Sleep 200ms
Type "rm -f ~/.ember/profile.toml"
Enter
Sleep 200ms
Type "clear"
Enter
Sleep 500ms
Show

# ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
# PART 1: Onboarding
# ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
Type "ember chat"
Sleep 300ms
Enter
Sleep 6s

# Q1: Name
Sleep 500ms
Type "Niklas"
Sleep 300ms
Enter
Sleep 1.5s

# Q2: Language — English default
Sleep 500ms
Enter
Sleep 1.5s

# Q3: Role
Sleep 500ms
Type "Full-Stack Developer"
Sleep 300ms
Enter
Sleep 1.5s

# Q4: Experience — Down to Expert
Sleep 500ms
Down
Sleep 300ms
Enter
Sleep 1.5s

# Q5: Favorite tech
Sleep 500ms
Type "Rust, TypeScript"
Sleep 300ms
Enter
Sleep 1.5s

# Q6: Communication style — Concise default
Sleep 500ms
Enter
Sleep 1.5s

# Egg hatching animation
Sleep 11s

# Name the buddy
Sleep 500ms
Type "Blaze"
Sleep 300ms
Enter
Sleep 1.5s

# Personality — Down 2x to "Witty & Sarcastic"
Sleep 500ms
Down
Sleep 300ms
Down
Sleep 300ms
Enter
Sleep 3s

# ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
# PART 2: Exit chat
# ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
Sleep 2s
Ctrl+C
Sleep 500ms
Ctrl+C
Sleep 2s

# ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
# PART 3: Open TUI and ask a question
# ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
Type "ember tui"
Sleep 300ms
Enter
Sleep 4s

Type "Can you create a file called hello.rs with a colorful greeting and then run it?"
Sleep 300ms
Enter

# Wait for tool execution
Sleep 25s

# Exit TUI
Ctrl+C
Sleep 2s
