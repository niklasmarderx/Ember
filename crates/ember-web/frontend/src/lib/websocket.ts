/**
 * Robust WebSocket Client with Reconnection Support
 *
 * Features:
 * - Exponential backoff with jitter (prevents thundering herd)
 * - Message queue during disconnection
 * - Automatic state synchronization after reconnection
 * - Heartbeat/ping-pong for connection health monitoring
 * - Sequence number tracking for deduplication
 */

// =============================================================================
// Types
// =============================================================================

/** Connection state enum matching backend */
export type ConnectionState =
  | 'disconnected'
  | 'connecting'
  | 'connected'
  | 'reconnecting'
  | 'failed'
  | 'closed';

/** Configuration for the WebSocket client */
export interface WebSocketConfig {
  /** WebSocket URL to connect to */
  url: string;
  /** Initial backoff delay in ms (default: 100) */
  initialBackoff?: number;
  /** Maximum backoff delay in ms (default: 30000 = 30s) */
  maxBackoff?: number;
  /** Backoff multiplier (default: 2.0) */
  backoffMultiplier?: number;
  /** Maximum retry attempts (default: 10, undefined = unlimited) */
  maxRetries?: number;
  /** Heartbeat interval in ms (default: 30000 = 30s) */
  heartbeatInterval?: number;
  /** Heartbeat timeout in ms (default: 10000 = 10s) */
  heartbeatTimeout?: number;
  /** Maximum outbound queue size (default: 100) */
  maxQueueSize?: number;
  /** Message timeout in ms (default: 60000 = 60s) */
  messageTimeout?: number;
  /** Callback when a message is received */
  onMessage?: (message: ServerMessage) => void;
  /** Callback when connection state changes */
  onStateChange?: (state: ConnectionState) => void;
  /** Callback when an error occurs */
  onError?: (error: Error) => void;
  /** Enable debug logging */
  debug?: boolean;
}

/** Queued message waiting to be sent */
interface QueuedMessage {
  payload: ClientMessage;
  queuedAt: number;
}

// =============================================================================
// Client Messages (to Server)
// =============================================================================

export interface StartChatMessage {
  type: 'start_chat';
  message: string;
  model?: string;
  conversation_id?: string;
  system_prompt?: string;
  temperature?: number;
  max_tokens?: number;
}

export interface PingMessage {
  type: 'ping';
  timestamp_ms?: number;
}

export interface SyncStateMessage {
  type: 'sync_state';
  last_received_seq: number;
  session_id?: string;
}

export interface AckMessage {
  type: 'ack';
  seq: number;
}

export interface SubscribeMessage {
  type: 'subscribe';
  channel: string;
}

export interface UnsubscribeMessage {
  type: 'unsubscribe';
  channel: string;
}

export interface PauseStreamMessage {
  type: 'pause_stream';
}

export interface ResumeStreamMessage {
  type: 'resume_stream';
}

export interface CancelStreamMessage {
  type: 'cancel_stream';
}

export type ClientMessage =
  | StartChatMessage
  | PingMessage
  | SyncStateMessage
  | AckMessage
  | SubscribeMessage
  | UnsubscribeMessage
  | PauseStreamMessage
  | ResumeStreamMessage
  | CancelStreamMessage;

// =============================================================================
// Server Messages (from Server)
// =============================================================================

export interface ConnectedMessage {
  type: 'connected';
  session_id: string;
  current_seq: number;
  heartbeat_interval_ms: number;
}

export interface StreamStartMessage {
  type: 'stream_start';
  seq?: number;
  stream_id: string;
  model: string;
  conversation_id: string;
}

export interface TokenMessage {
  type: 'token';
  seq?: number;
  stream_id: string;
  content: string;
  index: number;
  timestamp_ms: number;
}

export interface ProgressMessage {
  type: 'progress';
  seq?: number;
  stream_id: string;
  tokens_generated: number;
  elapsed_ms: number;
  tokens_per_second: number;
}

export interface StreamPausedMessage {
  type: 'stream_paused';
  seq?: number;
  stream_id: string;
}

export interface StreamResumedMessage {
  type: 'stream_resumed';
  seq?: number;
  stream_id: string;
}

export interface StreamCompleteMessage {
  type: 'stream_complete';
  seq?: number;
  stream_id: string;
  total_tokens: number;
  total_duration_ms: number;
  finish_reason: string;
}

export interface StreamCancelledMessage {
  type: 'stream_cancelled';
  seq?: number;
  stream_id: string;
}

export interface ErrorMessage {
  type: 'error';
  seq?: number;
  stream_id?: string;
  code: string;
  message: string;
}

export interface PongMessage {
  type: 'pong';
  timestamp_ms: number;
  client_timestamp_ms?: number;
}

export interface SystemNotificationMessage {
  type: 'system_notification';
  seq?: number;
  channel: string;
  payload: string;
}

export interface SyncStateResponseMessage {
  type: 'sync_state_response';
  replayed_count: number;
  current_seq: number;
  session_resumed: boolean;
  gaps?: [number, number][];
}

export interface ConnectionStateChangedMessage {
  type: 'connection_state_changed';
  state: ConnectionState;
  reason?: string;
}

export interface HeartbeatWarningMessage {
  type: 'heartbeat_warning';
  ms_since_last_pong: number;
}

export type ServerMessage =
  | ConnectedMessage
  | StreamStartMessage
  | TokenMessage
  | ProgressMessage
  | StreamPausedMessage
  | StreamResumedMessage
  | StreamCompleteMessage
  | StreamCancelledMessage
  | ErrorMessage
  | PongMessage
  | SystemNotificationMessage
  | SyncStateResponseMessage
  | ConnectionStateChangedMessage
  | HeartbeatWarningMessage;

// =============================================================================
// RobustWebSocket Class
// =============================================================================

/**
 * A robust WebSocket client with automatic reconnection, message queuing,
 * and state synchronization.
 */
export class RobustWebSocket {
  private ws: WebSocket | null = null;
  private config: Required<
    Omit<WebSocketConfig, 'onMessage' | 'onStateChange' | 'onError'>
  > & {
    onMessage?: (message: ServerMessage) => void;
    onStateChange?: (state: ConnectionState) => void;
    onError?: (error: Error) => void;
  };

  // Reconnection state
  private reconnectAttempts = 0;
  private currentBackoff: number;
  private reconnectTimer?: ReturnType<typeof setTimeout>;

  // Heartbeat state
  private heartbeatTimer?: ReturnType<typeof setInterval>;
  private lastPongReceived: number = Date.now();
  private heartbeatCheckTimer?: ReturnType<typeof setInterval>;

  // Message queue
  private outboundQueue: QueuedMessage[] = [];

  // State tracking
  private _state: ConnectionState = 'disconnected';
  private sessionId: string | null = null;
  private lastReceivedSeq = 0;

  // RTT tracking
  private lastRtt: number | null = null;

  constructor(config: WebSocketConfig) {
    this.config = {
      url: config.url,
      initialBackoff: config.initialBackoff ?? 100,
      maxBackoff: config.maxBackoff ?? 30000,
      backoffMultiplier: config.backoffMultiplier ?? 2.0,
      maxRetries: config.maxRetries ?? 10,
      heartbeatInterval: config.heartbeatInterval ?? 30000,
      heartbeatTimeout: config.heartbeatTimeout ?? 10000,
      maxQueueSize: config.maxQueueSize ?? 100,
      messageTimeout: config.messageTimeout ?? 60000,
      onMessage: config.onMessage,
      onStateChange: config.onStateChange,
      onError: config.onError,
      debug: config.debug ?? false,
    };
    this.currentBackoff = this.config.initialBackoff;
  }

  // ===========================================================================
  // Public API
  // ===========================================================================

  /** Current connection state */
  get state(): ConnectionState {
    return this._state;
  }

  /** Current session ID (assigned by server) */
  get currentSessionId(): string | null {
    return this.sessionId;
  }

  /** Last measured round-trip time in ms */
  get rtt(): number | null {
    return this.lastRtt;
  }

  /** Number of messages in outbound queue */
  get queuedMessageCount(): number {
    return this.outboundQueue.length;
  }

  /** Connect to the WebSocket server */
  connect(): void {
    if (this._state === 'connected' || this._state === 'connecting') {
      this.log('Already connected or connecting');
      return;
    }

    this.cleanup();
    this.setState('connecting');

    try {
      this.ws = new WebSocket(this.config.url);
      this.setupEventHandlers();
    } catch (error) {
      this.log('Connection error:', error);
      this.handleConnectionError(error as Error);
    }
  }

  /** Disconnect from the server */
  disconnect(): void {
    this.cleanup();
    this.setState('closed');
    this.ws?.close(1000, 'Client disconnect');
    this.ws = null;
  }

  /** Send a message to the server */
  send(message: ClientMessage): boolean {
    if (this.ws?.readyState === WebSocket.OPEN) {
      try {
        this.ws.send(JSON.stringify(message));
        return true;
      } catch (error) {
        this.log('Send error:', error);
        this.queueMessage(message);
        return false;
      }
    } else {
      // Queue message for later
      this.queueMessage(message);
      return false;
    }
  }

  /** Start a chat stream */
  startChat(
    message: string,
    options?: {
      model?: string;
      conversationId?: string;
      systemPrompt?: string;
      temperature?: number;
      maxTokens?: number;
    }
  ): boolean {
    return this.send({
      type: 'start_chat',
      message,
      model: options?.model,
      conversation_id: options?.conversationId,
      system_prompt: options?.systemPrompt,
      temperature: options?.temperature,
      max_tokens: options?.maxTokens,
    });
  }

  /** Cancel the current stream */
  cancelStream(): boolean {
    return this.send({ type: 'cancel_stream' });
  }

  /** Pause the current stream */
  pauseStream(): boolean {
    return this.send({ type: 'pause_stream' });
  }

  /** Resume a paused stream */
  resumeStream(): boolean {
    return this.send({ type: 'resume_stream' });
  }

  // ===========================================================================
  // Connection Management
  // ===========================================================================

  private setupEventHandlers(): void {
    if (!this.ws) return;

    this.ws.onopen = () => {
      this.log('WebSocket opened');
      // Wait for 'connected' message from server before transitioning state
    };

    this.ws.onclose = (event) => {
      this.log('WebSocket closed:', event.code, event.reason);
      this.stopHeartbeat();

      if (event.wasClean && event.code === 1000) {
        this.setState('closed');
      } else if (this._state !== 'closed') {
        this.scheduleReconnect();
      }
    };

    this.ws.onerror = (event) => {
      this.log('WebSocket error:', event);
      this.config.onError?.(new Error('WebSocket error'));
    };

    this.ws.onmessage = (event) => {
      this.handleMessage(event.data);
    };
  }

  private handleMessage(data: string): void {
    try {
      const message: ServerMessage = JSON.parse(data);
      this.log('Received:', message.type);

      // Track sequence numbers
      if ('seq' in message && message.seq !== undefined) {
        this.lastReceivedSeq = Math.max(this.lastReceivedSeq, message.seq);
      }

      // Handle specific message types
      switch (message.type) {
        case 'connected':
          this.handleConnected(message);
          break;
        case 'pong':
          this.handlePong(message);
          break;
        case 'sync_state_response':
          this.handleSyncResponse(message);
          break;
        default:
          // Forward to user callback
          this.config.onMessage?.(message);
      }
    } catch (error) {
      this.log('Error parsing message:', error);
    }
  }

  private handleConnected(message: ConnectedMessage): void {
    this.sessionId = message.session_id;
    this.reconnectAttempts = 0;
    this.currentBackoff = this.config.initialBackoff;

    this.setState('connected');

    // Start heartbeat with server-specified interval
    this.startHeartbeat(message.heartbeat_interval_ms);

    // Flush queued messages
    this.flushMessageQueue();

    // Request state sync if we have a previous sequence
    if (this.lastReceivedSeq > 0) {
      this.requestStateSync();
    }

    // Forward to user callback
    this.config.onMessage?.(message);
  }

  private handlePong(message: PongMessage): void {
    this.lastPongReceived = Date.now();

    // Calculate RTT if we sent a timestamp
    if (message.client_timestamp_ms !== undefined) {
      this.lastRtt = Date.now() - message.client_timestamp_ms;
      this.log('RTT:', this.lastRtt, 'ms');
    }
  }

  private handleSyncResponse(message: SyncStateResponseMessage): void {
    this.log(
      `State sync complete: ${message.replayed_count} messages replayed, session_resumed=${message.session_resumed}`
    );
    this.config.onMessage?.(message);
  }

  // ===========================================================================
  // Reconnection
  // ===========================================================================

  private scheduleReconnect(): void {
    // Check max retries
    if (
      this.config.maxRetries !== undefined &&
      this.reconnectAttempts >= this.config.maxRetries
    ) {
      this.log('Max retries exceeded');
      this.setState('failed');
      this.config.onError?.(new Error('Max reconnection attempts exceeded'));
      return;
    }

    this.reconnectAttempts++;
    this.setState('reconnecting');

    // Calculate backoff with jitter (0-25%)
    const jitter = this.currentBackoff * Math.random() * 0.25;
    const delay = this.currentBackoff + jitter;

    this.log(`Reconnecting in ${Math.round(delay)}ms (attempt ${this.reconnectAttempts})`);

    this.reconnectTimer = setTimeout(() => {
      this.connect();
    }, delay);

    // Exponential backoff with cap
    this.currentBackoff = Math.min(
      this.currentBackoff * this.config.backoffMultiplier,
      this.config.maxBackoff
    );
  }

  private handleConnectionError(error: Error): void {
    this.config.onError?.(error);
    if (this._state !== 'closed') {
      this.scheduleReconnect();
    }
  }

  // ===========================================================================
  // Heartbeat
  // ===========================================================================

  private startHeartbeat(intervalMs?: number): void {
    this.stopHeartbeat();

    const interval = intervalMs ?? this.config.heartbeatInterval;
    this.lastPongReceived = Date.now();

    // Send ping at regular intervals
    this.heartbeatTimer = setInterval(() => {
      if (this.ws?.readyState === WebSocket.OPEN) {
        this.send({
          type: 'ping',
          timestamp_ms: Date.now(),
        });
      }
    }, interval);

    // Check for heartbeat timeout
    this.heartbeatCheckTimer = setInterval(() => {
      const msSinceLastPong = Date.now() - this.lastPongReceived;
      if (msSinceLastPong > this.config.heartbeatTimeout) {
        this.log('Heartbeat timeout, closing connection');
        this.ws?.close(4000, 'Heartbeat timeout');
      }
    }, this.config.heartbeatTimeout / 2);
  }

  private stopHeartbeat(): void {
    if (this.heartbeatTimer) {
      clearInterval(this.heartbeatTimer);
      this.heartbeatTimer = undefined;
    }
    if (this.heartbeatCheckTimer) {
      clearInterval(this.heartbeatCheckTimer);
      this.heartbeatCheckTimer = undefined;
    }
  }

  // ===========================================================================
  // Message Queue
  // ===========================================================================

  private queueMessage(message: ClientMessage): void {
    // Prune old messages
    this.pruneQueue();

    // Check queue size limit
    while (this.outboundQueue.length >= this.config.maxQueueSize) {
      const dropped = this.outboundQueue.shift();
      this.log('Dropped oldest queued message:', dropped?.payload.type);
    }

    this.outboundQueue.push({
      payload: message,
      queuedAt: Date.now(),
    });

    this.log(`Message queued (${this.outboundQueue.length} in queue)`);
  }

  private pruneQueue(): void {
    const now = Date.now();
    const timeout = this.config.messageTimeout;

    this.outboundQueue = this.outboundQueue.filter(
      (msg) => now - msg.queuedAt < timeout
    );
  }

  private flushMessageQueue(): void {
    // Prune expired messages first
    this.pruneQueue();

    // Send all remaining messages
    const toSend = [...this.outboundQueue];
    this.outboundQueue = [];

    for (const msg of toSend) {
      if (this.ws?.readyState === WebSocket.OPEN) {
        try {
          this.ws.send(JSON.stringify(msg.payload));
          this.log('Flushed queued message:', msg.payload.type);
        } catch (error) {
          // Re-queue if send fails
          this.outboundQueue.push(msg);
        }
      } else {
        // Re-queue if not connected
        this.outboundQueue.push(msg);
      }
    }

    this.log(`Flushed ${toSend.length - this.outboundQueue.length} messages`);
  }

  // ===========================================================================
  // State Sync
  // ===========================================================================

  private requestStateSync(): void {
    this.log(`Requesting state sync from seq ${this.lastReceivedSeq}`);
    this.send({
      type: 'sync_state',
      last_received_seq: this.lastReceivedSeq,
      session_id: this.sessionId ?? undefined,
    });
  }

  // ===========================================================================
  // Utilities
  // ===========================================================================

  private setState(state: ConnectionState): void {
    if (this._state !== state) {
      this.log(`State: ${this._state} -> ${state}`);
      this._state = state;
      this.config.onStateChange?.(state);
    }
  }

  private cleanup(): void {
    this.stopHeartbeat();
    if (this.reconnectTimer) {
      clearTimeout(this.reconnectTimer);
      this.reconnectTimer = undefined;
    }
  }

  private log(...args: unknown[]): void {
    if (this.config.debug) {
      console.log('[RobustWebSocket]', ...args);
    }
  }
}

// =============================================================================
// React Hook
// =============================================================================

import { useCallback, useEffect, useRef, useState } from 'react';

/** Hook options */
export interface UseWebSocketOptions extends Omit<WebSocketConfig, 'url'> {
  /** Auto-connect on mount */
  autoConnect?: boolean;
}

/** Hook return type */
export interface UseWebSocketReturn {
  /** Current connection state */
  state: ConnectionState;
  /** Connect to the server */
  connect: () => void;
  /** Disconnect from the server */
  disconnect: () => void;
  /** Send a message */
  send: (message: ClientMessage) => boolean;
  /** Start a chat stream */
  startChat: (
    message: string,
    options?: {
      model?: string;
      conversationId?: string;
      systemPrompt?: string;
      temperature?: number;
      maxTokens?: number;
    }
  ) => boolean;
  /** Cancel the current stream */
  cancelStream: () => boolean;
  /** Last received message */
  lastMessage: ServerMessage | null;
  /** Session ID */
  sessionId: string | null;
  /** Round-trip time in ms */
  rtt: number | null;
  /** Number of queued messages */
  queuedCount: number;
}

/**
 * React hook for using the robust WebSocket client
 */
export function useWebSocket(
  url: string,
  options?: UseWebSocketOptions
): UseWebSocketReturn {
  const [state, setState] = useState<ConnectionState>('disconnected');
  const [lastMessage, setLastMessage] = useState<ServerMessage | null>(null);
  const [sessionId, setSessionId] = useState<string | null>(null);
  const [rtt, setRtt] = useState<number | null>(null);
  const [queuedCount, setQueuedCount] = useState(0);

  const wsRef = useRef<RobustWebSocket | null>(null);

  // Create WebSocket instance
  useEffect(() => {
    const ws = new RobustWebSocket({
      url,
      ...options,
      onMessage: (message) => {
        setLastMessage(message);
        if (message.type === 'connected') {
          setSessionId(message.session_id);
        }
        options?.onMessage?.(message);
      },
      onStateChange: (newState) => {
        setState(newState);
        options?.onStateChange?.(newState);
      },
      onError: options?.onError,
    });

    wsRef.current = ws;

    // Auto-connect if enabled
    if (options?.autoConnect !== false) {
      ws.connect();
    }

    // Update queued count periodically
    const queueInterval = setInterval(() => {
      if (wsRef.current) {
        setQueuedCount(wsRef.current.queuedMessageCount);
        setRtt(wsRef.current.rtt);
      }
    }, 1000);

    return () => {
      clearInterval(queueInterval);
      ws.disconnect();
    };
  }, [url]); // Only recreate when URL changes

  const connect = useCallback(() => {
    wsRef.current?.connect();
  }, []);

  const disconnect = useCallback(() => {
    wsRef.current?.disconnect();
  }, []);

  const send = useCallback((message: ClientMessage): boolean => {
    return wsRef.current?.send(message) ?? false;
  }, []);

  const startChat = useCallback(
    (
      message: string,
      chatOptions?: {
        model?: string;
        conversationId?: string;
        systemPrompt?: string;
        temperature?: number;
        maxTokens?: number;
      }
    ): boolean => {
      return wsRef.current?.startChat(message, chatOptions) ?? false;
    },
    []
  );

  const cancelStream = useCallback((): boolean => {
    return wsRef.current?.cancelStream() ?? false;
  }, []);

  return {
    state,
    connect,
    disconnect,
    send,
    startChat,
    cancelStream,
    lastMessage,
    sessionId,
    rtt,
    queuedCount,
  };
}