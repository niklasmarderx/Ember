import {
  BarChart3,
  Bot,
  Flame,
  Loader2,
  Menu,
  MessageSquare,
  Plus,
  Send,
  Settings,
  Trash2,
  User,
  X,
} from 'lucide-react';
import React, { useCallback, useEffect, useRef, useState } from 'react';
import ReactMarkdown from 'react-markdown';
import { Message } from './components/ChatMessage';
import CostDashboard from './components/CostDashboard';
import { ErrorBoundary, StreamingErrorBoundary } from './components/ErrorBoundary';
import { Conversation, useStore } from './store/useStore';

interface ServerInfo {
  name: string;
  version: string;
  llm_provider: string;
  default_model: string;
}

interface Model {
  id: string;
  name: string;
  provider: string;
}

export default function App() {
  // Use Zustand store for state management
  const {
    conversations,
    currentConversationId,
    isStreaming,
    settings,
    sidebarOpen,
    settingsOpen,
    getCurrentConversation,
    createConversation,
    selectConversation,
    deleteConversation,
    addMessage,
    setStreaming,
    toggleSidebar,
    toggleSettings,
    updateSettings,
    clearConversations,
    addCost,
  } = useStore();

  // Local state for server info and models (not persisted)
  const [serverInfo, setServerInfo] = useState<ServerInfo | null>(null);
  const [models, setModels] = useState<Model[]>([]);
  const [input, setInput] = useState('');
  const [activeView, setActiveView] = useState<'chat' | 'dashboard'>('chat');
  const [streamingContent, setStreamingContent] = useState('');

  const messagesEndRef = useRef<HTMLDivElement>(null);
  const textareaRef = useRef<HTMLTextAreaElement>(null);

  const currentConversation = getCurrentConversation();
  const messages = currentConversation?.messages || [];

  // Fetch server info and models on mount
  useEffect(() => {
    fetchServerInfo();
    fetchModels();
  }, []);

  // Scroll to bottom when messages change
  useEffect(() => {
    messagesEndRef.current?.scrollIntoView({ behavior: 'smooth' });
  }, [messages, streamingContent]);

  // Auto-resize textarea
  useEffect(() => {
    if (textareaRef.current) {
      textareaRef.current.style.height = 'auto';
      textareaRef.current.style.height = `${Math.min(textareaRef.current.scrollHeight, 200)}px`;
    }
  }, [input]);

  const fetchServerInfo = async () => {
    try {
      const res = await fetch('/api/v1/info');
      const data = await res.json();
      setServerInfo(data);
      if (!settings.defaultModel) {
        updateSettings({ defaultModel: data.default_model });
      }
    } catch (err) {
      console.error('Failed to fetch server info:', err);
    }
  };

  const fetchModels = async () => {
    try {
      const res = await fetch('/api/v1/models');
      const data = await res.json();
      setModels(data.models);
    } catch (err) {
      console.error('Failed to fetch models:', err);
    }
  };

  const sendMessage = useCallback(async () => {
    if (!input.trim() || isStreaming) return;

    // Create conversation if none exists
    let conversationId = currentConversationId;
    if (!conversationId) {
      conversationId = createConversation(settings.defaultModel, settings.defaultProvider);
    }

    // Add user message
    addMessage(conversationId, {
      role: 'user',
      content: input.trim(),
    });

    setInput('');
    setStreaming(true);
    setStreamingContent('');

    try {
      const response = await fetch('/api/v1/chat/stream', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          message: input.trim(),
          model: settings.defaultModel,
          messages: messages.map((m: Message) => ({ role: m.role, content: m.content })),
        }),
      });

      if (!response.ok) {
        throw new Error(`Request failed: ${response.status}`);
      }

      const reader = response.body?.getReader();
      if (!reader) {
        throw new Error('No reader available');
      }

      const decoder = new TextDecoder();
      let fullContent = '';
      let totalTokens = { input: 0, output: 0 };
      let cost = 0;

      while (true) {
        const { done, value } = await reader.read();
        if (done) break;

        const text = decoder.decode(value);
        const lines = text.split('\n');

        for (const line of lines) {
          if (line.startsWith('data: ')) {
            try {
              const data = JSON.parse(line.slice(6));
              if (data.event === 'chunk' && data.content) {
                fullContent += data.content;
                setStreamingContent(fullContent);
              } else if (data.event === 'done') {
                if (data.usage) {
                  totalTokens = {
                    input: data.usage.prompt_tokens || 0,
                    output: data.usage.completion_tokens || 0,
                  };
                }
                if (data.cost) {
                  cost = data.cost;
                }
              } else if (data.event === 'error') {
                throw new Error(data.error);
              }
            } catch {
              // Skip invalid JSON lines
            }
          }
        }
      }

      // Add assistant message
      addMessage(conversationId, {
        role: 'assistant',
        content: fullContent,
        tokens: totalTokens,
        cost,
      });

      // Track cost
      if (cost > 0) {
        addCost(cost);
      }

      setStreamingContent('');
    } catch (err) {
      console.error('Chat error:', err);
      addMessage(conversationId, {
        role: 'assistant',
        content: `Error: ${err instanceof Error ? err.message : 'Unknown error occurred'}`,
      });
    } finally {
      setStreaming(false);
    }
  }, [
    input,
    isStreaming,
    currentConversationId,
    messages,
    settings,
    createConversation,
    addMessage,
    setStreaming,
    addCost,
  ]);

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      sendMessage();
    }
  };

  const handleNewChat = () => {
    createConversation(settings.defaultModel, settings.defaultProvider);
  };

  const handleDeleteConversation = (id: string, e: React.MouseEvent) => {
    e.stopPropagation();
    deleteConversation(id);
  };

  return (
    <ErrorBoundary>
      <div className="flex h-screen bg-gray-900">
        {/* Sidebar */}
        <aside
          className={`${
            sidebarOpen ? 'w-64' : 'w-0'
          } flex-shrink-0 bg-gray-800 border-r border-gray-700 transition-all duration-300 overflow-hidden`}
          role="navigation"
          aria-label="Conversation sidebar"
        >
          <div className="flex flex-col h-full w-64">
            {/* New Chat Button */}
            <div className="p-3 border-b border-gray-700">
              <button
                onClick={handleNewChat}
                className="flex items-center justify-center gap-2 w-full px-4 py-2 bg-orange-700 hover:bg-orange-600 text-white rounded-lg transition-colors"
                aria-label="Start new chat"
              >
                <Plus className="w-4 h-4" aria-hidden="true" />
                New Chat
              </button>
            </div>

            {/* Conversation List */}
            <nav
              className="flex-1 overflow-y-auto p-2 space-y-1"
              aria-label="Conversations"
            >
              {conversations.length === 0 ? (
                <p className="text-gray-500 text-sm text-center py-4">
                  No conversations yet
                </p>
              ) : (
                conversations.map((conv: Conversation) => (
                  <button
                    key={conv.id}
                    onClick={() => selectConversation(conv.id)}
                    className={`group flex items-center justify-between w-full px-3 py-2 text-left rounded-lg transition-colors ${
                      conv.id === currentConversationId
                        ? 'bg-gray-700 text-white'
                        : 'text-gray-400 hover:bg-gray-700/50 hover:text-white'
                    }`}
                    aria-current={conv.id === currentConversationId ? 'page' : undefined}
                  >
                    <span className="truncate text-sm">{conv.title}</span>
                    <button
                      onClick={(e) => handleDeleteConversation(conv.id, e)}
                      className="opacity-0 group-hover:opacity-100 p-1 hover:text-red-400 transition-opacity"
                      aria-label={`Delete conversation: ${conv.title}`}
                    >
                      <Trash2 className="w-4 h-4" aria-hidden="true" />
                    </button>
                  </button>
                ))
              )}
            </nav>

            {/* Clear All */}
            {conversations.length > 0 && (
              <div className="p-3 border-t border-gray-700">
                <button
                  onClick={clearConversations}
                  className="flex items-center justify-center gap-2 w-full px-3 py-2 text-gray-400 hover:text-red-400 text-sm transition-colors"
                  aria-label="Clear all conversations"
                >
                  <Trash2 className="w-4 h-4" aria-hidden="true" />
                  Clear All
                </button>
              </div>
            )}
          </div>
        </aside>

        {/* Main Content */}
        <main className="flex-1 flex flex-col min-w-0">
          {/* Header */}
          <header className="flex items-center justify-between px-4 py-3 bg-gray-800 border-b border-gray-700">
            <div className="flex items-center gap-3">
              <button
                onClick={toggleSidebar}
                className="p-2 text-gray-400 hover:text-white hover:bg-gray-700 rounded-lg transition-colors lg:hidden"
                aria-label={sidebarOpen ? 'Close sidebar' : 'Open sidebar'}
                aria-expanded={sidebarOpen}
              >
                {sidebarOpen ? (
                  <X className="w-5 h-5" aria-hidden="true" />
                ) : (
                  <Menu className="w-5 h-5" aria-hidden="true" />
                )}
              </button>
              <Flame className="w-8 h-8 text-orange-600" aria-hidden="true" />
              <div>
                <h1 className="text-xl font-bold text-white">Ember AI</h1>
                {serverInfo && (
                  <p className="text-xs text-gray-400">
                    v{serverInfo.version} | {serverInfo.llm_provider}
                  </p>
                )}
              </div>
            </div>

            {/* Navigation Tabs */}
            <nav
              className="flex items-center gap-1 bg-gray-700/50 rounded-lg p-1"
              role="tablist"
              aria-label="Main navigation"
            >
              <button
                onClick={() => setActiveView('chat')}
                className={`flex items-center gap-2 px-3 py-1.5 rounded-md text-sm font-medium transition-colors ${
                  activeView === 'chat'
                    ? 'bg-orange-700 text-white'
                    : 'text-gray-400 hover:text-white'
                }`}
                role="tab"
                aria-selected={activeView === 'chat'}
                aria-controls="chat-panel"
                id="chat-tab"
              >
                <MessageSquare className="w-4 h-4" aria-hidden="true" />
                <span className="hidden sm:inline">Chat</span>
              </button>
              <button
                onClick={() => setActiveView('dashboard')}
                className={`flex items-center gap-2 px-3 py-1.5 rounded-md text-sm font-medium transition-colors ${
                  activeView === 'dashboard'
                    ? 'bg-orange-700 text-white'
                    : 'text-gray-400 hover:text-white'
                }`}
                role="tab"
                aria-selected={activeView === 'dashboard'}
                aria-controls="dashboard-panel"
                id="dashboard-tab"
              >
                <BarChart3 className="w-4 h-4" aria-hidden="true" />
                <span className="hidden sm:inline">Dashboard</span>
              </button>
            </nav>

            <div className="flex items-center gap-2">
              {activeView === 'chat' && (
                <>
                  <label htmlFor="model-select" className="sr-only">
                    Select Model
                  </label>
                  <select
                    id="model-select"
                    value={settings.defaultModel}
                    onChange={(e) => updateSettings({ defaultModel: e.target.value })}
                    className="px-3 py-1.5 text-sm bg-gray-700 border border-gray-600 rounded-lg text-white focus:outline-none focus:ring-2 focus:ring-orange-600"
                  >
                    {models.map((model) => (
                      <option key={model.id} value={model.id}>
                        {model.name}
                      </option>
                    ))}
                  </select>
                  <button
                    onClick={toggleSettings}
                    className="p-2 text-gray-400 hover:text-white hover:bg-gray-700 rounded-lg transition-colors"
                    aria-label="Open settings"
                    aria-expanded={settingsOpen}
                  >
                    <Settings className="w-5 h-5" aria-hidden="true" />
                  </button>
                </>
              )}
            </div>
          </header>

          {/* Dashboard View */}
          {activeView === 'dashboard' && (
            <div
              id="dashboard-panel"
              role="tabpanel"
              aria-labelledby="dashboard-tab"
              className="flex-1 overflow-hidden"
            >
              <ErrorBoundary>
                <CostDashboard />
              </ErrorBoundary>
            </div>
          )}

          {/* Chat View */}
          {activeView === 'chat' && (
            <div
              id="chat-panel"
              role="tabpanel"
              aria-labelledby="chat-tab"
              className="flex-1 flex flex-col min-h-0"
            >
              {/* Messages */}
              <div
                className="flex-1 overflow-y-auto p-4 space-y-4"
                role="log"
                aria-live="polite"
                aria-label="Chat messages"
              >
                {messages.length === 0 && !streamingContent && (
                  <div className="flex flex-col items-center justify-center h-full text-gray-500">
                    <Flame
                      className="w-16 h-16 mb-4 text-orange-600/50"
                      aria-hidden="true"
                    />
                    <p className="text-lg">Start a conversation with Ember</p>
                    <p className="text-sm mt-2">Type a message below to begin</p>
                  </div>
                )}

                {messages.map((message: Message) => (
                  <div
                    key={message.id}
                    className={`flex gap-3 ${
                      message.role === 'user' ? 'justify-end' : 'justify-start'
                    }`}
                  >
                    {message.role === 'assistant' && (
                      <div
                        className="flex-shrink-0 w-8 h-8 rounded-full bg-orange-700/20 flex items-center justify-center"
                        aria-hidden="true"
                      >
                        <Bot className="w-5 h-5 text-orange-600" />
                      </div>
                    )}
                    <div
                      className={`max-w-[80%] px-4 py-3 rounded-2xl ${
                        message.role === 'user'
                          ? 'bg-orange-700 text-white'
                          : 'bg-gray-800 text-gray-100'
                      }`}
                    >
                      {message.role === 'assistant' ? (
                        <div className="markdown-content prose prose-invert max-w-none">
                          <ReactMarkdown>{message.content}</ReactMarkdown>
                        </div>
                      ) : (
                        <p className="whitespace-pre-wrap">{message.content}</p>
                      )}
                      {message.tokens && settings.showTokenCounts && (
                        <p className="text-xs text-gray-500 mt-2">
                          Tokens: {message.tokens.input + message.tokens.output}
                        </p>
                      )}
                    </div>
                    {message.role === 'user' && (
                      <div
                        className="flex-shrink-0 w-8 h-8 rounded-full bg-gray-700 flex items-center justify-center"
                        aria-hidden="true"
                      >
                        <User className="w-5 h-5 text-gray-300" />
                      </div>
                    )}
                  </div>
                ))}

                {/* Streaming message */}
                {streamingContent && (
                  <StreamingErrorBoundary onRetry={sendMessage}>
                    <div className="flex gap-3 justify-start">
                      <div
                        className="flex-shrink-0 w-8 h-8 rounded-full bg-orange-700/20 flex items-center justify-center"
                        aria-hidden="true"
                      >
                        <Bot className="w-5 h-5 text-orange-600" />
                      </div>
                      <div className="max-w-[80%] px-4 py-3 rounded-2xl bg-gray-800 text-gray-100">
                        <div className="markdown-content prose prose-invert max-w-none">
                          <ReactMarkdown>{streamingContent}</ReactMarkdown>
                        </div>
                      </div>
                    </div>
                  </StreamingErrorBoundary>
                )}

                {/* Loading indicator */}
                {isStreaming && !streamingContent && (
                  <div className="flex gap-3 justify-start" aria-live="polite">
                    <div
                      className="flex-shrink-0 w-8 h-8 rounded-full bg-orange-700/20 flex items-center justify-center"
                      aria-hidden="true"
                    >
                      <Bot className="w-5 h-5 text-orange-600" />
                    </div>
                    <div className="px-4 py-3 rounded-2xl bg-gray-800">
                      <Loader2
                        className="w-5 h-5 text-orange-600 animate-spin"
                        aria-label="Loading response"
                      />
                    </div>
                  </div>
                )}

                <div ref={messagesEndRef} />
              </div>

              {/* Input */}
              <div className="p-4 border-t border-gray-700 bg-gray-800">
                <div className="flex gap-3 max-w-4xl mx-auto">
                  <label htmlFor="chat-input" className="sr-only">
                    Type your message
                  </label>
                  <textarea
                    id="chat-input"
                    ref={textareaRef}
                    value={input}
                    onChange={(e) => setInput(e.target.value)}
                    onKeyDown={handleKeyDown}
                    placeholder="Type your message..."
                    rows={1}
                    disabled={isStreaming}
                    className="flex-1 px-4 py-3 bg-gray-700 border border-gray-600 rounded-xl text-white placeholder-gray-400 resize-none focus:outline-none focus:ring-2 focus:ring-orange-600 focus:border-transparent disabled:opacity-50"
                    style={{ minHeight: '48px', maxHeight: '200px' }}
                    aria-describedby="input-hint"
                  />
                  <button
                    onClick={sendMessage}
                    disabled={!input.trim() || isStreaming}
                    className="px-4 py-3 bg-orange-700 hover:bg-orange-600 disabled:bg-gray-600 disabled:cursor-not-allowed text-white rounded-xl transition-colors flex items-center justify-center"
                    aria-label="Send message"
                  >
                    {isStreaming ? (
                      <Loader2
                        className="w-5 h-5 animate-spin"
                        aria-hidden="true"
                      />
                    ) : (
                      <Send className="w-5 h-5" aria-hidden="true" />
                    )}
                  </button>
                </div>
                <p
                  id="input-hint"
                  className="text-center text-xs text-gray-500 mt-2"
                >
                  Press Enter to send, Shift+Enter for new line
                </p>
              </div>
            </div>
          )}
        </main>
      </div>
    </ErrorBoundary>
  );
}