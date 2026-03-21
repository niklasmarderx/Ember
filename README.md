<div align="center">

<img src="assets/logo.svg" alt="Ember Logo" width="128" height="128"/>

# Ember

**Ein AI-Agent-Framework in Rust. Schnell, klein, laeuft ueberall.**

[![Website](https://img.shields.io/badge/website-ember.dev-orange)](https://niklasmarderx.github.io/Ember/)
[![Crates.io](https://img.shields.io/crates/v/ember-cli)](https://crates.io/crates/ember-cli)
[![License](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE-MIT)
[![CI](https://github.com/niklasmarderx/Ember/actions/workflows/ci.yml/badge.svg)](https://github.com/niklasmarderx/Ember/actions)

</div>

---

## Was ist Ember?

Ember ist ein Kommandozeilen-Tool und Framework, mit dem du KI-Modelle nutzen kannst - zum Chatten, fuer Code-Generierung oder um Aufgaben auf deinem Computer zu automatisieren.

Das Besondere: Ember ist in Rust geschrieben und kommt als einzelne ausfuehrbare Datei. Kein Python, kein Node.js, keine Abhaengigkeiten. Du laeadst eine Datei herunter, und es funktioniert.

---

## Schnellstart

### Mit Cloud-APIs (OpenAI, Anthropic, etc.)

```bash
curl -fsSL https://ember.dev/install.sh | sh
export OPENAI_API_KEY="sk-..."
ember chat
```

### Komplett offline und kostenlos

```bash
# Ollama installieren (einmalig)
curl -fsSL https://ollama.ai/install.sh | sh
ollama pull llama3.2

# Ember installieren und nutzen
curl -fsSL https://ember.dev/install.sh | sh
ember chat --provider ollama
```

### Als Docker-Container

```bash
docker run -it --rm ghcr.io/niklasmarderx/Ember chat "Hallo!"
```

### Web-Oberflaeche

```bash
ember serve
# Oeffne http://localhost:3000 im Browser
```

---

## Warum Ember?

### Ein Binary, keine Abhaengigkeiten

Ember kompiliert zu einer einzigen 15 MB grossen Datei. Du kopierst sie auf einen Server, einen Raspberry Pi oder deinen Laptop - und es laeuft. Keine `pip install`, keine Versionskonflikte, keine `node_modules`.

### Schnell

Rust-Programme starten sofort. Ember braucht etwa 80ms zum Starten, nicht mehrere Sekunden wie Python-basierte Tools. Der Speicherverbrauch liegt bei ca. 45 MB statt mehreren hundert MB.

### Funktioniert offline

Mit Ollama kannst du lokale Modelle wie Llama, Qwen oder Mistral nutzen. Komplett ohne Internet, ohne API-Kosten, ohne dass deine Daten irgendwohin geschickt werden.

### Viele Anbieter, ein Interface

Ember unterstuetzt OpenAI, Anthropic, Google Gemini, Mistral, Groq, DeepSeek, xAI, OpenRouter und Ollama. Du wechselst den Anbieter mit einem Flag, der Code bleibt gleich.

---

## Unterstuetzte LLM-Anbieter

| Anbieter | Beispiel-Modelle | Kosten |
|----------|------------------|--------|
| OpenAI | GPT-4o, GPT-4o-mini, o1 | Kostenpflichtig |
| Anthropic | Claude 3.5 Sonnet, Haiku | Kostenpflichtig |
| Google Gemini | Gemini 2.0, 1.5 Pro | Gratis-Kontingent |
| Groq | Llama 3.3 70B, Mixtral | Gratis-Kontingent |
| DeepSeek | V3, R1 | Guenstig |
| Mistral | Large, Codestral | Kostenpflichtig |
| xAI | Grok 2 | Kostenpflichtig |
| OpenRouter | 200+ Modelle | Variiert |
| Ollama | Llama, Qwen, etc. | Kostenlos (lokal) |

---

## Was kann Ember?

### Chat und Code-Generierung

```bash
ember chat "Erklaer mir Rekursion"
ember chat "Schreib eine Python-Funktion, die Primzahlen findet"
```

### Tools aktivieren

Ember kann Befehle ausfuehren, Dateien lesen und schreiben, Git nutzen und im Web suchen:

```bash
ember chat --tools shell,fs "Erstelle einen neuen Ordner 'projekt' und initialisiere Git"
ember chat --tools web "Was ist der aktuelle Bitcoin-Preis?"
```

### Web-Oberflaeche

Die Web-UI zeigt Chat-Verlaeufe, Kosten-Tracking und laesst dich zwischen Modellen wechseln.

### Checkpoints

Ember speichert jeden Schritt. Du kannst jederzeit zurueckgehen, wenn etwas schiefgeht.

### Kosten-Tracking

Bei Cloud-Anbietern siehst du in Echtzeit, was ein Chat kostet. Du kannst Budget-Limits setzen.

---

## Installation

### Ein Befehl

```bash
curl -fsSL https://ember.dev/install.sh | sh
```

### Mit Cargo (wenn du Rust installiert hast)

```bash
cargo install ember-cli
```

### Mit Homebrew (macOS/Linux)

```bash
brew install ember-agent
```

### Aus dem Quellcode

```bash
git clone https://github.com/niklasmarderx/Ember
cd Ember
cargo build --release
```

---

## Konfiguration

Ember liest API-Keys aus Umgebungsvariablen:

```bash
# OpenAI
export OPENAI_API_KEY="sk-..."

# Anthropic
export ANTHROPIC_API_KEY="..."

# Fuer andere Anbieter siehe die Dokumentation
```

Oder leg eine `.env`-Datei an:

```
OPENAI_API_KEY=sk-...
EMBER_DEFAULT_PROVIDER=openai
EMBER_DEFAULT_MODEL=gpt-4o-mini
```

---

## Beispiele

### Einfacher Chat

```rust
use ember::prelude::*;

#[tokio::main]
async fn main() -> Result<()> {
    let agent = Agent::builder()
        .provider(OpenAIProvider::from_env()?)
        .build()?;
    
    let antwort = agent.chat("Was ist die Hauptstadt von Frankreich?").await?;
    println!("{}", antwort);
    Ok(())
}
```

### Mit Tools

```rust
use ember::prelude::*;

#[tokio::main]
async fn main() -> Result<()> {
    let agent = Agent::builder()
        .provider(OllamaProvider::new()?)
        .tool(tools::Shell::new())
        .tool(tools::Filesystem::sandboxed("./workspace"))
        .build()?;
    
    agent.chat("Liste alle .rs Dateien im aktuellen Verzeichnis").await?;
    Ok(())
}
```

---

## Projektstruktur

```
ember/
├── crates/
│   ├── ember-core/      # Agent, Memory, Konfiguration
│   ├── ember-llm/       # LLM-Anbieter
│   ├── ember-tools/     # Shell, Dateisystem, Git, Web
│   ├── ember-storage/   # SQLite, Vektor-DB, RAG
│   ├── ember-cli/       # Kommandozeile
│   ├── ember-web/       # Web-Server und React-Frontend
│   └── ...
├── examples/            # Code-Beispiele
├── docs/                # Dokumentation
└── extensions/          # VS Code Extension
```

---

## Dokumentation

- [Erste Schritte](https://ember.dev/docs/getting-started)
- [CLI-Referenz](https://ember.dev/docs/cli)
- [Anbieter konfigurieren](https://ember.dev/docs/providers)
- [Eigene Tools bauen](https://ember.dev/docs/custom-tools)
- [API-Dokumentation (Rust)](https://docs.rs/ember)

---

## Mithelfen

Beitraege sind willkommen. Schau dir [CONTRIBUTING.md](CONTRIBUTING.md) an fuer Details.

```bash
git clone https://github.com/niklasmarderx/Ember
cd Ember
cargo test --workspace
cargo run -p ember-cli -- chat "Test"
```

---

## Lizenz

MIT - siehe [LICENSE-MIT](LICENSE-MIT)

---

<div align="center">

**Fragen?** [niklas.marder@gmail.com](mailto:niklas.marder@gmail.com)

[![GitHub](https://img.shields.io/github/stars/niklasmarderx/Ember?style=social)](https://github.com/niklasmarderx/Ember)

</div>