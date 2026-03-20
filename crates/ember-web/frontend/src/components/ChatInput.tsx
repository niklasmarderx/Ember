import React, { useCallback, useEffect, useRef, useState } from 'react';

interface ChatInputProps {
  onSend: (message: string, options?: SendOptions) => void;
  disabled?: boolean;
  placeholder?: string;
  maxLength?: number;
  showToolsToggle?: boolean;
  enabledTools?: string[];
  onToolsChange?: (tools: string[]) => void;
  isStreaming?: boolean;
  onStop?: () => void;
}

interface SendOptions {
  tools?: string[];
  model?: string;
}

const AVAILABLE_TOOLS = [
  { id: 'shell', name: 'Shell', icon: 'terminal', description: 'Execute commands' },
  { id: 'filesystem', name: 'Files', icon: 'folder', description: 'Read/write files' },
  { id: 'web', name: 'Web', icon: 'globe', description: 'HTTP requests' },
  { id: 'browser', name: 'Browser', icon: 'browser', description: 'Browser automation' },
  { id: 'git', name: 'Git', icon: 'git', description: 'Git operations' },
  { id: 'code', name: 'Code', icon: 'code', description: 'Execute code' },
];

export const ChatInput: React.FC<ChatInputProps> = ({
  onSend,
  disabled = false,
  placeholder = 'Type a message... (Ctrl+Enter to send)',
  maxLength = 100000,
  showToolsToggle = true,
  enabledTools = [],
  onToolsChange,
  isStreaming = false,
  onStop,
}) => {
  const [message, setMessage] = useState('');
  const [showTools, setShowTools] = useState(false);
  const [selectedTools, setSelectedTools] = useState<string[]>(enabledTools);
  const [rows, setRows] = useState(1);
  const textareaRef = useRef<HTMLTextAreaElement>(null);
  const toolsRef = useRef<HTMLDivElement>(null);

  // Auto-resize textarea
  useEffect(() => {
    if (textareaRef.current) {
      textareaRef.current.style.height = 'auto';
      const scrollHeight = textareaRef.current.scrollHeight;
      const newRows = Math.min(Math.max(Math.ceil(scrollHeight / 24), 1), 10);
      setRows(newRows);
      textareaRef.current.style.height = `${scrollHeight}px`;
    }
  }, [message]);

  // Close tools dropdown when clicking outside
  useEffect(() => {
    const handleClickOutside = (event: MouseEvent) => {
      if (toolsRef.current && !toolsRef.current.contains(event.target as Node)) {
        setShowTools(false);
      }
    };
    document.addEventListener('mousedown', handleClickOutside);
    return () => document.removeEventListener('mousedown', handleClickOutside);
  }, []);

  // Focus textarea on mount
  useEffect(() => {
    textareaRef.current?.focus();
  }, []);

  const handleSend = useCallback(() => {
    const trimmed = message.trim();
    if (trimmed && !disabled) {
      onSend(trimmed, { tools: selectedTools });
      setMessage('');
      if (textareaRef.current) {
        textareaRef.current.style.height = 'auto';
      }
      setRows(1);
    }
  }, [message, disabled, onSend, selectedTools]);

  const handleKeyDown = (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
    // Ctrl+Enter or Cmd+Enter to send
    if ((e.ctrlKey || e.metaKey) && e.key === 'Enter') {
      e.preventDefault();
      handleSend();
    }
    // Escape to clear
    if (e.key === 'Escape') {
      setMessage('');
    }
  };

  const toggleTool = (toolId: string) => {
    const newTools = selectedTools.includes(toolId)
      ? selectedTools.filter((t) => t !== toolId)
      : [...selectedTools, toolId];
    setSelectedTools(newTools);
    onToolsChange?.(newTools);
  };

  const charCount = message.length;
  const isOverLimit = charCount > maxLength;

  return (
    <div className="border-t border-gray-200 dark:border-gray-700 bg-white dark:bg-gray-900 p-4">
      {/* Tools Selection Bar */}
      {selectedTools.length > 0 && (
        <div className="mb-2 flex items-center gap-2 text-xs">
          <span className="text-gray-500 dark:text-gray-400">Active tools:</span>
          {selectedTools.map((toolId) => {
            const tool = AVAILABLE_TOOLS.find((t) => t.id === toolId);
            return tool ? (
              <span
                key={toolId}
                className="px-2 py-0.5 bg-purple-100 dark:bg-purple-900/30 text-purple-700 dark:text-purple-300 rounded-full flex items-center gap-1"
              >
                {tool.name}
                <button
                  onClick={() => toggleTool(toolId)}
                  className="hover:text-purple-900 dark:hover:text-purple-100"
                >
                  x
                </button>
              </span>
            ) : null;
          })}
        </div>
      )}

      <div className="flex items-end gap-2">
        {/* Tools Toggle */}
        {showToolsToggle && (
          <div className="relative" ref={toolsRef}>
            <button
              onClick={() => setShowTools(!showTools)}
              className={`p-2 rounded-lg transition-colors ${
                selectedTools.length > 0
                  ? 'bg-purple-100 dark:bg-purple-900/30 text-purple-600 dark:text-purple-400'
                  : 'hover:bg-gray-100 dark:hover:bg-gray-800 text-gray-500 dark:text-gray-400'
              }`}
              title="Toggle tools"
            >
              <svg className="w-5 h-5" fill="currentColor" viewBox="0 0 20 20">
                <path
                  fillRule="evenodd"
                  d="M11.49 3.17c-.38-1.56-2.6-1.56-2.98 0a1.532 1.532 0 01-2.286.948c-1.372-.836-2.942.734-2.106 2.106.54.886.061 2.042-.947 2.287-1.561.379-1.561 2.6 0 2.978a1.532 1.532 0 01.947 2.287c-.836 1.372.734 2.942 2.106 2.106a1.532 1.532 0 012.287.947c.379 1.561 2.6 1.561 2.978 0a1.533 1.533 0 012.287-.947c1.372.836 2.942-.734 2.106-2.106a1.533 1.533 0 01.947-2.287c1.561-.379 1.561-2.6 0-2.978a1.532 1.532 0 01-.947-2.287c.836-1.372-.734-2.942-2.106-2.106a1.532 1.532 0 01-2.287-.947zM10 13a3 3 0 100-6 3 3 0 000 6z"
                  clipRule="evenodd"
                />
              </svg>
            </button>

            {/* Tools Dropdown */}
            {showTools && (
              <div className="absolute bottom-full left-0 mb-2 w-64 bg-white dark:bg-gray-800 rounded-lg shadow-lg border border-gray-200 dark:border-gray-700 p-2 z-10">
                <div className="text-xs font-medium text-gray-500 dark:text-gray-400 mb-2 px-2">
                  Available Tools
                </div>
                {AVAILABLE_TOOLS.map((tool) => (
                  <button
                    key={tool.id}
                    onClick={() => toggleTool(tool.id)}
                    className={`w-full flex items-center gap-3 px-2 py-2 rounded-lg transition-colors ${
                      selectedTools.includes(tool.id)
                        ? 'bg-purple-100 dark:bg-purple-900/30 text-purple-700 dark:text-purple-300'
                        : 'hover:bg-gray-100 dark:hover:bg-gray-700 text-gray-700 dark:text-gray-300'
                    }`}
                  >
                    <div
                      className={`w-4 h-4 rounded border flex items-center justify-center ${
                        selectedTools.includes(tool.id)
                          ? 'bg-purple-600 border-purple-600'
                          : 'border-gray-300 dark:border-gray-600'
                      }`}
                    >
                      {selectedTools.includes(tool.id) && (
                        <svg className="w-3 h-3 text-white" fill="currentColor" viewBox="0 0 20 20">
                          <path
                            fillRule="evenodd"
                            d="M16.707 5.293a1 1 0 010 1.414l-8 8a1 1 0 01-1.414 0l-4-4a1 1 0 011.414-1.414L8 12.586l7.293-7.293a1 1 0 011.414 0z"
                            clipRule="evenodd"
                          />
                        </svg>
                      )}
                    </div>
                    <div className="flex-1 text-left">
                      <div className="font-medium text-sm">{tool.name}</div>
                      <div className="text-xs text-gray-500 dark:text-gray-400">
                        {tool.description}
                      </div>
                    </div>
                  </button>
                ))}
              </div>
            )}
          </div>
        )}

        {/* Textarea */}
        <div className="flex-1 relative">
          <textarea
            ref={textareaRef}
            value={message}
            onChange={(e) => setMessage(e.target.value)}
            onKeyDown={handleKeyDown}
            placeholder={placeholder}
            disabled={disabled || isStreaming}
            rows={rows}
            maxLength={maxLength + 1000} // Allow some overflow for warning
            className={`w-full px-4 py-3 rounded-lg border transition-colors resize-none ${
              isOverLimit
                ? 'border-red-500 focus:ring-red-500'
                : 'border-gray-300 dark:border-gray-600 focus:ring-blue-500'
            } dark:bg-gray-800 focus:outline-none focus:ring-2 focus:border-transparent disabled:opacity-50 disabled:cursor-not-allowed`}
            aria-label="Chat message input"
          />
          {/* Character count */}
          {charCount > maxLength * 0.9 && (
            <div
              className={`absolute bottom-2 right-2 text-xs ${
                isOverLimit ? 'text-red-500' : 'text-gray-400'
              }`}
            >
              {charCount}/{maxLength}
            </div>
          )}
        </div>

        {/* Send/Stop Button */}
        {isStreaming ? (
          <button
            onClick={onStop}
            className="p-3 bg-red-600 text-white rounded-lg hover:bg-red-700 transition-colors"
            title="Stop generation"
          >
            <svg className="w-5 h-5" fill="currentColor" viewBox="0 0 20 20">
              <path
                fillRule="evenodd"
                d="M10 18a8 8 0 100-16 8 8 0 000 16zM8 7a1 1 0 00-1 1v4a1 1 0 001 1h4a1 1 0 001-1V8a1 1 0 00-1-1H8z"
                clipRule="evenodd"
              />
            </svg>
          </button>
        ) : (
          <button
            onClick={handleSend}
            disabled={disabled || !message.trim() || isOverLimit}
            className="p-3 bg-blue-600 text-white rounded-lg hover:bg-blue-700 transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
            title="Send message (Ctrl+Enter)"
          >
            <svg className="w-5 h-5" fill="currentColor" viewBox="0 0 20 20">
              <path d="M10.894 2.553a1 1 0 00-1.788 0l-7 14a1 1 0 001.169 1.409l5-1.429A1 1 0 009 15.571V11a1 1 0 112 0v4.571a1 1 0 00.725.962l5 1.428a1 1 0 001.17-1.408l-7-14z" />
            </svg>
          </button>
        )}
      </div>

      {/* Keyboard Shortcuts Hint */}
      <div className="mt-2 flex items-center justify-between text-xs text-gray-400 dark:text-gray-500">
        <div className="flex items-center gap-4">
          <span>
            <kbd className="px-1.5 py-0.5 bg-gray-100 dark:bg-gray-800 rounded text-xs">Ctrl</kbd>
            {' + '}
            <kbd className="px-1.5 py-0.5 bg-gray-100 dark:bg-gray-800 rounded text-xs">Enter</kbd>
            {' to send'}
          </span>
          <span>
            <kbd className="px-1.5 py-0.5 bg-gray-100 dark:bg-gray-800 rounded text-xs">Esc</kbd>
            {' to clear'}
          </span>
        </div>
        {selectedTools.length > 0 && (
          <span className="text-purple-500 dark:text-purple-400">
            Agent mode: {selectedTools.length} tool{selectedTools.length > 1 ? 's' : ''} enabled
          </span>
        )}
      </div>
    </div>
  );
};

export default ChatInput;