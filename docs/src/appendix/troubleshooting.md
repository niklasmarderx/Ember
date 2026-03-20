# Troubleshooting

Common issues and their solutions.

## Installation Issues

### Rust Compilation Errors

**Problem**: `cargo build` fails with compilation errors.

**Solutions**:
1. Ensure you have Rust 1.75 or later:
   ```bash
   rustup update stable
   rustc --version
   ```
2. Clear the build cache:
   ```bash
   cargo clean
   cargo build
   ```
3. Update dependencies:
   ```bash
   cargo update
   ```

### Missing System Dependencies

**Problem**: Build fails with linker errors.

**Linux (Ubuntu/Debian)**:
```bash
sudo apt-get install build-essential pkg-config libssl-dev
```

**macOS**:
```bash
xcode-select --install
```

**Windows**:
Install Visual Studio Build Tools with C++ workload.

## Runtime Issues

### API Key Not Found

**Problem**: `Error: API key not found for provider 'openai'`

**Solutions**:
1. Set environment variable:
   ```bash
   export OPENAI_API_KEY=sk-...
   ```
2. Create `.env` file:
   ```
   OPENAI_API_KEY=sk-...
   ```
3. Use config command:
   ```bash
   ember config set openai_api_key sk-...
   ```

### Connection Timeout

**Problem**: `Error: Request timeout after 30s`

**Solutions**:
1. Check internet connection
2. Increase timeout:
   ```bash
   ember config set timeout 60
   ```
3. Check if provider is accessible:
   ```bash
   curl https://api.openai.com/v1/models -H "Authorization: Bearer $OPENAI_API_KEY"
   ```

### Rate Limiting

**Problem**: `Error: Rate limit exceeded`

**Solutions**:
1. Wait and retry (Ember does this automatically)
2. Reduce concurrent requests
3. Use a different provider temporarily
4. Upgrade your API plan

## Tool Execution Issues

### Shell Command Blocked

**Problem**: `Error: Command 'rm' is not in allowed list`

**Solution**: Add command to allowed list:
```bash
ember config set allowed_commands "ls,cat,grep,rm"
```

Or use agent mode with explicit permissions:
```bash
ember interactive --allow-commands "rm,mv"
```

### File Access Denied

**Problem**: `Error: Path '/etc/passwd' is outside allowed directories`

**Solution**: Configure allowed paths:
```bash
ember config set allowed_paths "/home/user/project,/tmp"
```

### Browser Tool Not Working

**Problem**: `Error: Browser not found`

**Solutions**:
1. Install Chromium:
   ```bash
   # Linux
   sudo apt-get install chromium-browser
   
   # macOS
   brew install chromium
   ```
2. Set Chrome path:
   ```bash
   export CHROME_PATH=/path/to/chrome
   ```

## Memory and Storage Issues

### Database Locked

**Problem**: `Error: Database is locked`

**Solutions**:
1. Check for other Ember processes:
   ```bash
   ps aux | grep ember
   ```
2. Remove lock file:
   ```bash
   rm ~/.ember/db.lock
   ```
3. Use a different database path:
   ```bash
   ember config set database_path /tmp/ember.db
   ```

### Out of Memory

**Problem**: Ember crashes with OOM error.

**Solutions**:
1. Reduce context window size:
   ```bash
   ember config set max_context_tokens 8000
   ```
2. Clear conversation history:
   ```bash
   ember history clear
   ```
3. Disable memory features:
   ```bash
   ember config set memory_enabled false
   ```

## Provider-Specific Issues

### OpenAI

**401 Unauthorized**: Invalid API key
- Verify your key at https://platform.openai.com/api-keys

**429 Too Many Requests**: Rate limited
- Wait 60 seconds or upgrade plan

**500 Internal Server Error**: OpenAI issue
- Check https://status.openai.com

### Anthropic

**Invalid API Key**: Key format incorrect
- Anthropic keys start with `sk-ant-`

**Model Not Found**: Using unavailable model
- Check available models: `claude-3-opus-20240229`

### Ollama

**Connection Refused**: Ollama not running
```bash
ollama serve
```

**Model Not Found**: Model not pulled
```bash
ollama pull llama2
```

## Web UI Issues

### WebSocket Connection Failed

**Problem**: `WebSocket connection to 'ws://...' failed`

**Solutions**:
1. Check if server is running:
   ```bash
   ember serve
   ```
2. Check firewall settings
3. Use HTTP instead of HTTPS for local development

### CORS Errors

**Problem**: `Access-Control-Allow-Origin` errors

**Solution**: Enable CORS on the server:
```bash
ember serve --cors
```

## Performance Issues

### Slow Response Times

**Solutions**:
1. Use streaming:
   ```bash
   ember chat --stream "message"
   ```
2. Use a faster model:
   ```bash
   ember chat -m gpt-3.5-turbo "message"
   ```
3. Enable response caching:
   ```bash
   ember config set cache_enabled true
   ```

### High Memory Usage

**Solutions**:
1. Limit conversation history:
   ```bash
   ember config set max_history 10
   ```
2. Disable vector storage:
   ```bash
   ember config set rag_enabled false
   ```

## Getting More Help

### Debug Logging

Enable verbose logging:
```bash
RUST_LOG=debug ember chat "test"
```

### Report a Bug

1. Check existing issues: https://github.com/ember-ai/ember/issues
2. Create a new issue with:
   - Ember version (`ember --version`)
   - OS and version
   - Steps to reproduce
   - Error message and logs

### Community Support

- Discord: discord.gg/ember-ai
- GitHub Discussions: https://github.com/ember-ai/ember/discussions