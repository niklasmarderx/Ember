import React, { useState } from 'react';

export interface ToolCall {
  id: string;
  name: string;
  arguments: Record<string, unknown>;
  result?: string;
  error?: string;
  status: 'pending' | 'running' | 'success' | 'error' | 'cancelled';
  startTime?: Date;
  endTime?: Date;
  duration?: number;
}

interface ToolExecutionProps {
  toolCall: ToolCall;
  onCancel?: (id: string) => void;
  onRetry?: (id: string) => void;
  defaultExpanded?: boolean;
}

const TOOL_ICONS: Record<string, React.ReactNode> = {
  shell: (
    <svg className="w-4 h-4" fill="currentColor" viewBox="0 0 20 20">
      <path fillRule="evenodd" d="M2 5a2 2 0 012-2h12a2 2 0 012 2v10a2 2 0 01-2 2H4a2 2 0 01-2-2V5zm3.293 1.293a1 1 0 011.414 0l3 3a1 1 0 010 1.414l-3 3a1 1 0 01-1.414-1.414L7.586 10 5.293 7.707a1 1 0 010-1.414zM11 12a1 1 0 100 2h3a1 1 0 100-2h-3z" clipRule="evenodd" />
    </svg>
  ),
  filesystem: (
    <svg className="w-4 h-4" fill="currentColor" viewBox="0 0 20 20">
      <path d="M2 6a2 2 0 012-2h5l2 2h5a2 2 0 012 2v6a2 2 0 01-2 2H4a2 2 0 01-2-2V6z" />
    </svg>
  ),
  web: (
    <svg className="w-4 h-4" fill="currentColor" viewBox="0 0 20 20">
      <path fillRule="evenodd" d="M4.083 9h1.946c.089-1.546.383-2.97.837-4.118A6.004 6.004 0 004.083 9zM10 2a8 8 0 100 16 8 8 0 000-16zm0 2c-.076 0-.232.032-.465.262-.238.234-.497.623-.737 1.182-.389.907-.673 2.142-.766 3.556h3.936c-.093-1.414-.377-2.649-.766-3.556-.24-.56-.5-.948-.737-1.182C10.232 4.032 10.076 4 10 4zm3.971 5c-.089-1.546-.383-2.97-.837-4.118A6.004 6.004 0 0115.917 9h-1.946zm-2.003 2H8.032c.093 1.414.377 2.649.766 3.556.24.56.5.948.737 1.182.233.23.389.262.465.262.076 0 .232-.032.465-.262.238-.234.498-.623.737-1.182.389-.907.673-2.142.766-3.556zm1.166 4.118c.454-1.147.748-2.572.837-4.118h1.946a6.004 6.004 0 01-2.783 4.118zm-6.268 0C6.412 13.97 6.118 12.546 6.03 11H4.083a6.004 6.004 0 002.783 4.118z" clipRule="evenodd" />
    </svg>
  ),
  browser: (
    <svg className="w-4 h-4" fill="currentColor" viewBox="0 0 20 20">
      <path fillRule="evenodd" d="M3 5a2 2 0 012-2h10a2 2 0 012 2v8a2 2 0 01-2 2h-2.22l.123.489.804.804A1 1 0 0113 18H7a1 1 0 01-.707-1.707l.804-.804L7.22 15H5a2 2 0 01-2-2V5zm5.771 7H5V5h10v7H8.771z" clipRule="evenodd" />
    </svg>
  ),
  git: (
    <svg className="w-4 h-4" fill="currentColor" viewBox="0 0 20 20">
      <path fillRule="evenodd" d="M10 2a1 1 0 00-1 1v1.323l-3.954 1.582 1.599-.8a1 1 0 10-.894-1.79l-4 2A1 1 0 001 6.531V13a1 1 0 00.553.894l4 2a1 1 0 00.894-1.789l-1.599-.8L9 11.323V17a1 1 0 002 0v-5.677l4.152 1.662-1.599.8a1 1 0 00.894 1.789l4-2A1 1 0 0019 13V6.531a1 1 0 00-.553-.894l-4-2a1 1 0 00-.894 1.79l1.599.8L11 8.209V3a1 1 0 00-1-1z" clipRule="evenodd" />
    </svg>
  ),
  code: (
    <svg className="w-4 h-4" fill="currentColor" viewBox="0 0 20 20">
      <path fillRule="evenodd" d="M12.316 3.051a1 1 0 01.633 1.265l-4 12a1 1 0 11-1.898-.632l4-12a1 1 0 011.265-.633zM5.707 6.293a1 1 0 010 1.414L3.414 10l2.293 2.293a1 1 0 11-1.414 1.414l-3-3a1 1 0 010-1.414l3-3a1 1 0 011.414 0zm8.586 0a1 1 0 011.414 0l3 3a1 1 0 010 1.414l-3 3a1 1 0 11-1.414-1.414L16.586 10l-2.293-2.293a1 1 0 010-1.414z" clipRule="evenodd" />
    </svg>
  ),
  default: (
    <svg className="w-4 h-4" fill="currentColor" viewBox="0 0 20 20">
      <path fillRule="evenodd" d="M11.49 3.17c-.38-1.56-2.6-1.56-2.98 0a1.532 1.532 0 01-2.286.948c-1.372-.836-2.942.734-2.106 2.106.54.886.061 2.042-.947 2.287-1.561.379-1.561 2.6 0 2.978a1.532 1.532 0 01.947 2.287c-.836 1.372.734 2.942 2.106 2.106a1.532 1.532 0 012.287.947c.379 1.561 2.6 1.561 2.978 0a1.533 1.533 0 012.287-.947c1.372.836 2.942-.734 2.106-2.106a1.533 1.533 0 01.947-2.287c1.561-.379 1.561-2.6 0-2.978a1.532 1.532 0 01-.947-2.287c.836-1.372-.734-2.942-2.106-2.106a1.532 1.532 0 01-2.287-.947zM10 13a3 3 0 100-6 3 3 0 000 6z" clipRule="evenodd" />
    </svg>
  ),
};

const STATUS_STYLES = {
  pending: {
    bg: 'bg-gray-100 dark:bg-gray-800',
    border: 'border-gray-300 dark:border-gray-600',
    text: 'text-gray-600 dark:text-gray-400',
    badge: 'bg-gray-200 dark:bg-gray-700 text-gray-700 dark:text-gray-300',
  },
  running: {
    bg: 'bg-yellow-50 dark:bg-yellow-900/20',
    border: 'border-yellow-300 dark:border-yellow-700',
    text: 'text-yellow-700 dark:text-yellow-300',
    badge: 'bg-yellow-200 dark:bg-yellow-800 text-yellow-800 dark:text-yellow-200',
  },
  success: {
    bg: 'bg-green-50 dark:bg-green-900/20',
    border: 'border-green-300 dark:border-green-700',
    text: 'text-green-700 dark:text-green-300',
    badge: 'bg-green-200 dark:bg-green-800 text-green-800 dark:text-green-200',
  },
  error: {
    bg: 'bg-red-50 dark:bg-red-900/20',
    border: 'border-red-300 dark:border-red-700',
    text: 'text-red-700 dark:text-red-300',
    badge: 'bg-red-200 dark:bg-red-800 text-red-800 dark:text-red-200',
  },
  cancelled: {
    bg: 'bg-gray-50 dark:bg-gray-800',
    border: 'border-gray-300 dark:border-gray-600',
    text: 'text-gray-500 dark:text-gray-400',
    badge: 'bg-gray-200 dark:bg-gray-700 text-gray-600 dark:text-gray-400',
  },
};

export const ToolExecution: React.FC<ToolExecutionProps> = ({
  toolCall,
  onCancel,
  onRetry,
  defaultExpanded = false,
}) => {
  const [isExpanded, setIsExpanded] = useState(defaultExpanded);
  const [showFullResult, setShowFullResult] = useState(false);

  const styles = STATUS_STYLES[toolCall.status];
  const icon = TOOL_ICONS[toolCall.name] || TOOL_ICONS.default;

  const formatDuration = (ms: number) => {
    if (ms < 1000) return `${ms}ms`;
    if (ms < 60000) return `${(ms / 1000).toFixed(2)}s`;
    return `${Math.floor(ms / 60000)}m ${Math.floor((ms % 60000) / 1000)}s`;
  };

  const formatTime = (date: Date) => {
    return new Intl.DateTimeFormat('default', {
      hour: '2-digit',
      minute: '2-digit',
      second: '2-digit',
    }).format(date);
  };

  const truncateResult = (result: string, maxLength: number = 500) => {
    if (result.length <= maxLength) return result;
    return result.substring(0, maxLength) + '...';
  };

  const argsString = JSON.stringify(toolCall.arguments, null, 2);
  const resultString = toolCall.result || toolCall.error || '';
  const isTruncated = resultString.length > 500;

  return (
    <div className={`rounded-lg border ${styles.border} ${styles.bg} overflow-hidden transition-all duration-200`}>
      {/* Header */}
      <div
        className="flex items-center justify-between p-3 cursor-pointer hover:bg-black/5 dark:hover:bg-white/5"
        onClick={() => setIsExpanded(!isExpanded)}
      >
        <div className="flex items-center gap-3">
          {/* Tool Icon */}
          <span className={styles.text}>{icon}</span>

          {/* Tool Name */}
          <span className="font-medium text-gray-800 dark:text-gray-200">
            {toolCall.name}
          </span>

          {/* Status Badge */}
          <span className={`px-2 py-0.5 text-xs rounded-full ${styles.badge}`}>
            {toolCall.status === 'running' && (
              <span className="inline-block w-2 h-2 mr-1 bg-yellow-500 rounded-full animate-pulse" />
            )}
            {toolCall.status}
          </span>

          {/* Duration */}
          {toolCall.duration && (
            <span className="text-xs text-gray-500 dark:text-gray-400">
              {formatDuration(toolCall.duration)}
            </span>
          )}
        </div>

        <div className="flex items-center gap-2">
          {/* Time */}
          {toolCall.startTime && (
            <span className="text-xs text-gray-400 dark:text-gray-500">
              {formatTime(toolCall.startTime)}
            </span>
          )}

          {/* Actions */}
          {toolCall.status === 'running' && onCancel && (
            <button
              onClick={(e) => {
                e.stopPropagation();
                onCancel(toolCall.id);
              }}
              className="p-1 text-red-500 hover:bg-red-100 dark:hover:bg-red-900/30 rounded"
              title="Cancel"
            >
              <svg className="w-4 h-4" fill="currentColor" viewBox="0 0 20 20">
                <path fillRule="evenodd" d="M10 18a8 8 0 100-16 8 8 0 000 16zM8.707 7.293a1 1 0 00-1.414 1.414L8.586 10l-1.293 1.293a1 1 0 101.414 1.414L10 11.414l1.293 1.293a1 1 0 001.414-1.414L11.414 10l1.293-1.293a1 1 0 00-1.414-1.414L10 8.586 8.707 7.293z" clipRule="evenodd" />
              </svg>
            </button>
          )}

          {toolCall.status === 'error' && onRetry && (
            <button
              onClick={(e) => {
                e.stopPropagation();
                onRetry(toolCall.id);
              }}
              className="p-1 text-blue-500 hover:bg-blue-100 dark:hover:bg-blue-900/30 rounded"
              title="Retry"
            >
              <svg className="w-4 h-4" fill="currentColor" viewBox="0 0 20 20">
                <path fillRule="evenodd" d="M4 2a1 1 0 011 1v2.101a7.002 7.002 0 0111.601 2.566 1 1 0 11-1.885.666A5.002 5.002 0 005.999 7H9a1 1 0 010 2H4a1 1 0 01-1-1V3a1 1 0 011-1zm.008 9.057a1 1 0 011.276.61A5.002 5.002 0 0014.001 13H11a1 1 0 110-2h5a1 1 0 011 1v5a1 1 0 11-2 0v-2.101a7.002 7.002 0 01-11.601-2.566 1 1 0 01.61-1.276z" clipRule="evenodd" />
              </svg>
            </button>
          )}

          {/* Expand/Collapse */}
          <svg
            className={`w-4 h-4 text-gray-400 transition-transform ${isExpanded ? 'rotate-180' : ''}`}
            fill="currentColor"
            viewBox="0 0 20 20"
          >
            <path fillRule="evenodd" d="M5.293 7.293a1 1 0 011.414 0L10 10.586l3.293-3.293a1 1 0 111.414 1.414l-4 4a1 1 0 01-1.414 0l-4-4a1 1 0 010-1.414z" clipRule="evenodd" />
          </svg>
        </div>
      </div>

      {/* Expanded Content */}
      {isExpanded && (
        <div className="border-t border-gray-200 dark:border-gray-700">
          {/* Arguments */}
          <div className="p-3 border-b border-gray-200 dark:border-gray-700">
            <div className="text-xs font-medium text-gray-500 dark:text-gray-400 mb-2">
              Arguments
            </div>
            <pre className="text-xs font-mono text-gray-700 dark:text-gray-300 bg-gray-50 dark:bg-gray-900 p-2 rounded overflow-x-auto">
              {argsString}
            </pre>
          </div>

          {/* Result / Error */}
          {resultString && (
            <div className="p-3">
              <div className="flex items-center justify-between mb-2">
                <span className={`text-xs font-medium ${toolCall.error ? 'text-red-500' : 'text-gray-500 dark:text-gray-400'}`}>
                  {toolCall.error ? 'Error' : 'Result'}
                </span>
                {isTruncated && (
                  <button
                    onClick={() => setShowFullResult(!showFullResult)}
                    className="text-xs text-blue-500 hover:text-blue-600"
                  >
                    {showFullResult ? 'Show less' : 'Show more'}
                  </button>
                )}
              </div>
              <pre
                className={`text-xs font-mono p-2 rounded overflow-x-auto whitespace-pre-wrap ${
                  toolCall.error
                    ? 'text-red-600 dark:text-red-400 bg-red-50 dark:bg-red-900/20'
                    : 'text-gray-700 dark:text-gray-300 bg-gray-50 dark:bg-gray-900'
                }`}
              >
                {showFullResult ? resultString : truncateResult(resultString)}
              </pre>
            </div>
          )}

          {/* Running Indicator */}
          {toolCall.status === 'running' && (
            <div className="p-3 flex items-center gap-2 text-sm text-yellow-600 dark:text-yellow-400">
              <svg className="w-4 h-4 animate-spin" fill="none" viewBox="0 0 24 24">
                <circle className="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" strokeWidth="4" />
                <path className="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z" />
              </svg>
              Executing...
            </div>
          )}
        </div>
      )}
    </div>
  );
};

// Component for displaying multiple tool executions
interface ToolExecutionListProps {
  toolCalls: ToolCall[];
  onCancel?: (id: string) => void;
  onRetry?: (id: string) => void;
}

export const ToolExecutionList: React.FC<ToolExecutionListProps> = ({
  toolCalls,
  onCancel,
  onRetry,
}) => {
  if (toolCalls.length === 0) return null;

  const runningCount = toolCalls.filter((t) => t.status === 'running').length;
  const successCount = toolCalls.filter((t) => t.status === 'success').length;
  const errorCount = toolCalls.filter((t) => t.status === 'error').length;

  return (
    <div className="space-y-3">
      {/* Summary Header */}
      <div className="flex items-center justify-between text-sm">
        <span className="font-medium text-gray-700 dark:text-gray-300">
          Tool Executions ({toolCalls.length})
        </span>
        <div className="flex items-center gap-3 text-xs">
          {runningCount > 0 && (
            <span className="flex items-center gap-1 text-yellow-600 dark:text-yellow-400">
              <span className="w-2 h-2 bg-yellow-500 rounded-full animate-pulse" />
              {runningCount} running
            </span>
          )}
          {successCount > 0 && (
            <span className="text-green-600 dark:text-green-400">
              {successCount} completed
            </span>
          )}
          {errorCount > 0 && (
            <span className="text-red-600 dark:text-red-400">
              {errorCount} failed
            </span>
          )}
        </div>
      </div>

      {/* Tool Execution Cards */}
      <div className="space-y-2">
        {toolCalls.map((toolCall) => (
          <ToolExecution
            key={toolCall.id}
            toolCall={toolCall}
            onCancel={onCancel}
            onRetry={onRetry}
            defaultExpanded={toolCall.status === 'running' || toolCall.status === 'error'}
          />
        ))}
      </div>
    </div>
  );
};

export default ToolExecution;