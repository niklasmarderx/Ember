/**
 * Connection Status Component
 *
 * Displays the current WebSocket connection state with visual indicators
 * and optional detailed information (RTT, queued messages, session ID).
 */

import { AlertCircle, CheckCircle2, Loader2, RefreshCw, WifiOff, XCircle } from 'lucide-react';
import type { ConnectionState } from '../lib/websocket';

interface ConnectionStatusProps {
  /** Current connection state */
  state: ConnectionState;
  /** Session ID (if connected) */
  sessionId?: string | null;
  /** Round-trip time in ms */
  rtt?: number | null;
  /** Number of queued messages */
  queuedCount?: number;
  /** Show detailed info */
  showDetails?: boolean;
  /** Callback when retry is clicked (for failed state) */
  onRetry?: () => void;
  /** Additional CSS classes */
  className?: string;
}

/** Connection state configuration */
const stateConfig: Record<
  ConnectionState,
  {
    icon: React.ElementType;
    label: string;
    color: string;
    bgColor: string;
    animate?: boolean;
  }
> = {
  disconnected: {
    icon: WifiOff,
    label: 'Disconnected',
    color: 'text-gray-400',
    bgColor: 'bg-gray-500/20',
  },
  connecting: {
    icon: Loader2,
    label: 'Connecting...',
    color: 'text-blue-400',
    bgColor: 'bg-blue-500/20',
    animate: true,
  },
  connected: {
    icon: CheckCircle2,
    label: 'Connected',
    color: 'text-green-400',
    bgColor: 'bg-green-500/20',
  },
  reconnecting: {
    icon: RefreshCw,
    label: 'Reconnecting...',
    color: 'text-yellow-400',
    bgColor: 'bg-yellow-500/20',
    animate: true,
  },
  failed: {
    icon: XCircle,
    label: 'Connection Failed',
    color: 'text-red-400',
    bgColor: 'bg-red-500/20',
  },
  closed: {
    icon: AlertCircle,
    label: 'Closed',
    color: 'text-gray-400',
    bgColor: 'bg-gray-500/20',
  },
};

/**
 * Displays connection status with visual indicators
 */
export function ConnectionStatus({
  state,
  sessionId,
  rtt,
  queuedCount = 0,
  showDetails = false,
  onRetry,
  className = '',
}: ConnectionStatusProps) {
  const config = stateConfig[state];
  const Icon = config.icon;

  return (
    <div className={`flex items-center gap-2 ${className}`}>
      {/* Status Indicator */}
      <div
        className={`flex items-center gap-1.5 px-2 py-1 rounded-full ${config.bgColor}`}
        title={`Connection: ${config.label}`}
      >
        <Icon
          className={`w-3.5 h-3.5 ${config.color} ${config.animate ? 'animate-spin' : ''}`}
        />
        <span className={`text-xs font-medium ${config.color}`}>
          {config.label}
        </span>
      </div>

      {/* RTT Badge (when connected and RTT available) */}
      {state === 'connected' && rtt !== null && rtt !== undefined && (
        <div
          className="px-2 py-0.5 text-xs bg-gray-700/50 rounded-full text-gray-300"
          title="Round-trip time"
        >
          {rtt}ms
        </div>
      )}

      {/* Queued Messages Badge */}
      {queuedCount > 0 && (
        <div
          className="px-2 py-0.5 text-xs bg-yellow-600/30 rounded-full text-yellow-300"
          title={`${queuedCount} message${queuedCount !== 1 ? 's' : ''} queued`}
        >
          {queuedCount} queued
        </div>
      )}

      {/* Retry Button (for failed state) */}
      {state === 'failed' && onRetry && (
        <button
          onClick={onRetry}
          className="px-2 py-1 text-xs bg-red-600/30 hover:bg-red-600/50 rounded-md text-red-300 transition-colors"
        >
          Retry
        </button>
      )}

      {/* Detailed Info */}
      {showDetails && sessionId && (
        <div
          className="px-2 py-0.5 text-xs bg-gray-800/50 rounded text-gray-500 font-mono truncate max-w-[150px]"
          title={`Session: ${sessionId}`}
        >
          {sessionId.substring(0, 8)}...
        </div>
      )}
    </div>
  );
}

/**
 * Minimal connection indicator (just a dot)
 */
export function ConnectionDot({
  state,
  className = '',
}: {
  state: ConnectionState;
  className?: string;
}) {
  const config = stateConfig[state];

  const dotColors: Record<ConnectionState, string> = {
    disconnected: 'bg-gray-400',
    connecting: 'bg-blue-400',
    connected: 'bg-green-400',
    reconnecting: 'bg-yellow-400',
    failed: 'bg-red-400',
    closed: 'bg-gray-400',
  };

  return (
    <div
      className={`w-2 h-2 rounded-full ${dotColors[state]} ${
        config.animate ? 'animate-pulse' : ''
      } ${className}`}
      title={config.label}
    />
  );
}

/**
 * Connection status banner (for showing at top of page during issues)
 */
export function ConnectionBanner({
  state,
  queuedCount = 0,
  onRetry,
  onDismiss,
}: {
  state: ConnectionState;
  queuedCount?: number;
  onRetry?: () => void;
  onDismiss?: () => void;
}) {
  // Only show banner for problematic states
  if (state === 'connected' || state === 'closed') {
    return null;
  }

  const config = stateConfig[state];
  const Icon = config.icon;

  const bannerColors: Record<ConnectionState, string> = {
    disconnected: 'bg-gray-800 border-gray-700',
    connecting: 'bg-blue-900/50 border-blue-800',
    connected: '',
    reconnecting: 'bg-yellow-900/50 border-yellow-800',
    failed: 'bg-red-900/50 border-red-800',
    closed: '',
  };

  return (
    <div
      className={`fixed top-0 left-0 right-0 z-50 px-4 py-2 border-b ${bannerColors[state]} flex items-center justify-between`}
    >
      <div className="flex items-center gap-2">
        <Icon
          className={`w-4 h-4 ${config.color} ${config.animate ? 'animate-spin' : ''}`}
        />
        <span className={`text-sm ${config.color}`}>
          {state === 'reconnecting' && 'Connection lost. Attempting to reconnect...'}
          {state === 'connecting' && 'Connecting to server...'}
          {state === 'disconnected' && 'Not connected to server'}
          {state === 'failed' && 'Unable to connect to server'}
        </span>
        {queuedCount > 0 && (
          <span className="text-xs text-gray-400">
            ({queuedCount} message{queuedCount !== 1 ? 's' : ''} will be sent when connected)
          </span>
        )}
      </div>
      <div className="flex items-center gap-2">
        {state === 'failed' && onRetry && (
          <button
            onClick={onRetry}
            className="px-3 py-1 text-xs bg-red-600 hover:bg-red-500 rounded text-white transition-colors"
          >
            Try Again
          </button>
        )}
        {onDismiss && (
          <button
            onClick={onDismiss}
            className="p-1 hover:bg-white/10 rounded transition-colors"
            aria-label="Dismiss"
          >
            <XCircle className="w-4 h-4 text-gray-400" />
          </button>
        )}
      </div>
    </div>
  );
}

export default ConnectionStatus;