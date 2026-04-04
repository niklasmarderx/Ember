# Ember Init — Provider Setup Wizard Demo
# Run: vhs demo/init.tape

Output demo/init.gif

Require ember

Set Shell "bash"
Set FontSize 16
Set Width 900
Set Height 500
Set Padding 20
Set Theme "Catppuccin Mocha"
Set TypingSpeed 50ms
Set PlaybackSpeed 1

# Start the init wizard
Type "ember init --force"
Enter
Sleep 2s

# ── Step 1: Provider selection (arrow-key menu, default=OpenAI idx 0) ──
# Navigate to Ollama (Down twice: OpenAI -> Anthropic -> Ollama)
Down
Sleep 300ms
Down
Sleep 300ms
Enter
Sleep 1.5s

# ── Step 2: API key — Ollama skips this automatically ──
# ── Step 3: Model selection ──
# raw_input: "Model [llama3.2]:" — keep default
Enter
Sleep 1.5s

# ── Step 4: Test message? (Y/n) ──
# Skip test (Ollama might not be running on the recording machine)
Type "n"
Enter
Sleep 2s

# Show final output
Sleep 3s
