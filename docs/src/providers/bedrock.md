# AWS Bedrock

AWS Bedrock ist ein vollstaendig verwalteter Service, der Foundation Models (FMs) von fuehrenden KI-Unternehmen ueber eine einheitliche API anbietet.

## Unterstuetzte Modelle

Ember unterstuetzt folgende Model-Familien auf Bedrock:

### Anthropic Claude
- `anthropic.claude-3-opus-20240229-v1:0` - Claude 3 Opus (leistungsfaehigstes Modell)
- `anthropic.claude-3-sonnet-20240229-v1:0` - Claude 3 Sonnet (ausgewogen)
- `anthropic.claude-3-haiku-20240307-v1:0` - Claude 3 Haiku (schnell und effizient)

### Amazon Titan
- `amazon.titan-text-express-v1` - Titan Text Express
- `amazon.titan-text-lite-v1` - Titan Text Lite

### Meta Llama
- `meta.llama2-13b-chat-v1` - Llama 2 13B Chat
- `meta.llama2-70b-chat-v1` - Llama 2 70B Chat
- `meta.llama3-8b-instruct-v1:0` - Llama 3 8B Instruct
- `meta.llama3-70b-instruct-v1:0` - Llama 3 70B Instruct

### Mistral AI
- `mistral.mistral-7b-instruct-v0:2` - Mistral 7B Instruct
- `mistral.mixtral-8x7b-instruct-v0:1` - Mixtral 8x7B

### Cohere
- `cohere.command-text-v14` - Command

### AI21 Jurassic
- `ai21.j2-mid-v1` - Jurassic-2 Mid
- `ai21.j2-ultra-v1` - Jurassic-2 Ultra

## Konfiguration

### Umgebungsvariablen

```bash
# AWS Region (erforderlich)
export AWS_REGION="us-east-1"
# oder
export AWS_DEFAULT_REGION="us-east-1"

# AWS Credentials (optional, wenn IAM Rolle verwendet wird)
export AWS_ACCESS_KEY_ID="your-access-key"
export AWS_SECRET_ACCESS_KEY="your-secret-key"

# Optional: Session Token fuer temporaere Credentials
export AWS_SESSION_TOKEN="your-session-token"

# Optional: Standard-Modell
export BEDROCK_DEFAULT_MODEL="anthropic.claude-3-sonnet-20240229-v1:0"

# Optional: Benutzerdefinierter Endpoint
export BEDROCK_ENDPOINT_URL="https://custom-endpoint.example.com"
```

### Ember Konfiguration

In `~/.config/ember/config.toml`:

```toml
[providers.bedrock]
region = "us-east-1"
default_model = "anthropic.claude-3-sonnet-20240229-v1:0"
```

## Verwendung

### CLI

```bash
# Mit Bedrock-Provider
ember chat --provider bedrock "Hallo von AWS Bedrock!"

# Spezifisches Modell
ember chat --provider bedrock --model "amazon.titan-text-express-v1" "Erklaere mir Machine Learning"

# Claude 3 Opus fuer komplexe Aufgaben
ember chat --provider bedrock --model "anthropic.claude-3-opus-20240229-v1:0" "Analysiere diesen Code..."
```

### Rust API

```rust
use ember_llm::{BedrockProvider, BedrockConfig, CompletionRequest, Message, LLMProvider};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Von Umgebungsvariablen
    let provider = BedrockProvider::from_env()?;
    
    // Oder mit expliziter Konfiguration
    let config = BedrockConfig {
        region: "eu-west-1".to_string(),
        default_model: "anthropic.claude-3-sonnet-20240229-v1:0".to_string(),
        ..Default::default()
    };
    let provider = BedrockProvider::new(config);
    
    // Oder mit expliziten Credentials
    let provider = BedrockProvider::with_credentials(
        "eu-west-1",
        "your-access-key",
        "your-secret-key"
    );
    
    // Chat Request
    let request = CompletionRequest::new("anthropic.claude-3-sonnet-20240229-v1:0")
        .with_message(Message::user("Was ist AWS Bedrock?"));
    
    let response = provider.complete(request).await?;
    println!("{}", response.content);
    
    Ok(())
}
```

## Feature-Unterstuetzung

| Feature | Claude | Titan | Llama | Mistral | Cohere | Jurassic |
|---------|--------|-------|-------|---------|--------|----------|
| Chat | Ja | Ja | Ja | Ja | Ja | Ja |
| Streaming | Ja | Nein | Nein | Nein | Nein | Nein |
| Tools | Ja | Nein | Nein | Nein | Nein | Nein |
| Vision | Ja | Nein | Nein | Nein | Nein | Nein |

## AWS IAM Berechtigungen

Fuer die Nutzung von Bedrock benoetigen Sie entsprechende IAM-Berechtigungen:

```json
{
    "Version": "2012-10-17",
    "Statement": [
        {
            "Effect": "Allow",
            "Action": [
                "bedrock:InvokeModel",
                "bedrock:InvokeModelWithResponseStream"
            ],
            "Resource": "arn:aws:bedrock:*:*:foundation-model/*"
        }
    ]
}
```

## Regionale Verfuegbarkeit

Nicht alle Modelle sind in allen AWS-Regionen verfuegbar. Pruefen Sie die [AWS Bedrock Dokumentation](https://docs.aws.amazon.com/bedrock/latest/userguide/what-is-bedrock.html) fuer aktuelle Informationen.

Haeufig verfuegbare Regionen:
- `us-east-1` (N. Virginia)
- `us-west-2` (Oregon)
- `eu-west-1` (Ireland)
- `ap-northeast-1` (Tokyo)

## Fehlerbehebung

### Haeufige Fehler

**AccessDeniedException**
- Pruefen Sie Ihre IAM-Berechtigungen
- Stellen Sie sicher, dass das Modell in Ihrer Region aktiviert ist

**ResourceNotFoundException**
- Das angeforderte Modell ist nicht verfuegbar
- Pruefen Sie die Modell-ID und Region

**ThrottlingException**
- Rate Limit erreicht
- Implementieren Sie Retry-Logik oder reduzieren Sie die Anfragerate

### Debug-Modus

```bash
RUST_LOG=ember_llm=debug ember chat --provider bedrock "Test"
```

## Kosten

AWS Bedrock berechnet nach Input- und Output-Tokens. Preise variieren je nach Modell:

- Claude 3 Opus: Premium-Preise fuer komplexe Aufgaben
- Claude 3 Sonnet: Mittlere Preisstufe
- Claude 3 Haiku: Kostenguenstig fuer einfache Aufgaben
- Titan: Guenstige Amazon-eigene Modelle

Aktuelle Preise finden Sie in der [AWS Bedrock Pricing](https://aws.amazon.com/bedrock/pricing/) Seite.