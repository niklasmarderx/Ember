import { create } from 'zustand';
import { createJSONStorage, persist } from 'zustand/middleware';
import { Message } from '../components/ChatMessage';
import { ToolCall } from '../components/ToolExecution';

// Types
export interface Conversation {
  id: string;
  title: string;
  messages: Message[];
  createdAt: Date;
  updatedAt: Date;
  model: string;
  provider: string;
  tokens: {
    input: number;
    output: number;
  };
  cost: number;
  tags?: string[];
}

export interface Settings {
  theme: 'light' | 'dark' | 'system';
  defaultModel: string;
  defaultProvider: string;
  enabledTools: string[];
  maxTokens: number;
  temperature: number;
  streamResponses: boolean;
  showTokenCounts: boolean;
  showCosts: boolean;
  saveHistory: boolean;
  apiKeys: Record<string, string>;
  budgetLimit?: number;
  budgetPeriod: 'daily' | 'weekly' | 'monthly';
}

export interface BudgetUsage {
  daily: number;
  weekly: number;
  monthly: number;
  lastReset: {
    daily: string;
    weekly: string;
    monthly: string;
  };
}

interface AppState {
  // Conversations
  conversations: Conversation[];
  currentConversationId: string | null;
  
  // Messages & Streaming
  isStreaming: boolean;
  pendingToolCalls: ToolCall[];
  
  // Settings
  settings: Settings;
  
  // UI State
  sidebarOpen: boolean;
  settingsOpen: boolean;
  shortcutsOpen: boolean;
  exportDialogOpen: boolean;
  
  // Connection
  isConnected: boolean;
  reconnectAttempts: number;
  
  // Budget
  budgetUsage: BudgetUsage;
  
  // Actions - Conversations
  createConversation: (model?: string, provider?: string) => string;
  deleteConversation: (id: string) => void;
  selectConversation: (id: string | null) => void;
  updateConversationTitle: (id: string, title: string) => void;
  clearConversations: () => void;
  
  // Actions - Messages
  addMessage: (conversationId: string, message: Omit<Message, 'id' | 'timestamp'>) => void;
  updateMessage: (conversationId: string, messageId: string, updates: Partial<Message>) => void;
  deleteMessage: (conversationId: string, messageId: string) => void;
  
  // Actions - Streaming
  setStreaming: (isStreaming: boolean) => void;
  addPendingToolCall: (toolCall: ToolCall) => void;
  updateToolCall: (id: string, updates: Partial<ToolCall>) => void;
  clearPendingToolCalls: () => void;
  
  // Actions - Settings
  updateSettings: (settings: Partial<Settings>) => void;
  resetSettings: () => void;
  
  // Actions - UI
  toggleSidebar: () => void;
  toggleSettings: () => void;
  toggleShortcuts: () => void;
  toggleExportDialog: () => void;
  
  // Actions - Connection
  setConnected: (connected: boolean) => void;
  incrementReconnectAttempts: () => void;
  resetReconnectAttempts: () => void;
  
  // Actions - Budget
  addCost: (cost: number) => void;
  resetBudget: (period: 'daily' | 'weekly' | 'monthly') => void;
  
  // Computed
  getCurrentConversation: () => Conversation | undefined;
  getTotalCost: () => number;
  getTotalTokens: () => { input: number; output: number };
}

const DEFAULT_SETTINGS: Settings = {
  theme: 'system',
  defaultModel: 'gpt-4',
  defaultProvider: 'openai',
  enabledTools: [],
  maxTokens: 4096,
  temperature: 0.7,
  streamResponses: true,
  showTokenCounts: true,
  showCosts: true,
  saveHistory: true,
  apiKeys: {},
  budgetLimit: undefined,
  budgetPeriod: 'monthly',
};

const generateId = () => crypto.randomUUID();

const generateTitle = (messages: Message[]): string => {
  const firstUserMessage = messages.find(m => m.role === 'user');
  if (firstUserMessage) {
    const content = firstUserMessage.content.slice(0, 50);
    return content.length < firstUserMessage.content.length ? `${content}...` : content;
  }
  return 'New Conversation';
};

export const useStore = create<AppState>()(
  persist(
    (set, get) => ({
      // Initial State
      conversations: [],
      currentConversationId: null,
      isStreaming: false,
      pendingToolCalls: [],
      settings: DEFAULT_SETTINGS,
      sidebarOpen: true,
      settingsOpen: false,
      shortcutsOpen: false,
      exportDialogOpen: false,
      isConnected: false,
      reconnectAttempts: 0,
      budgetUsage: {
        daily: 0,
        weekly: 0,
        monthly: 0,
        lastReset: {
          daily: new Date().toISOString().split('T')[0],
          weekly: new Date().toISOString().split('T')[0],
          monthly: new Date().toISOString().slice(0, 7),
        },
      },

      // Conversation Actions
      createConversation: (model, provider) => {
        const id = generateId();
        const { settings } = get();
        const conversation: Conversation = {
          id,
          title: 'New Conversation',
          messages: [],
          createdAt: new Date(),
          updatedAt: new Date(),
          model: model || settings.defaultModel,
          provider: provider || settings.defaultProvider,
          tokens: { input: 0, output: 0 },
          cost: 0,
        };
        set((state) => ({
          conversations: [conversation, ...state.conversations],
          currentConversationId: id,
        }));
        return id;
      },

      deleteConversation: (id) => {
        set((state) => ({
          conversations: state.conversations.filter((c) => c.id !== id),
          currentConversationId:
            state.currentConversationId === id ? null : state.currentConversationId,
        }));
      },

      selectConversation: (id) => {
        set({ currentConversationId: id });
      },

      updateConversationTitle: (id, title) => {
        set((state) => ({
          conversations: state.conversations.map((c) =>
            c.id === id ? { ...c, title, updatedAt: new Date() } : c
          ),
        }));
      },

      clearConversations: () => {
        set({ conversations: [], currentConversationId: null });
      },

      // Message Actions
      addMessage: (conversationId, message) => {
        const newMessage: Message = {
          ...message,
          id: generateId(),
          timestamp: new Date(),
        };
        set((state) => ({
          conversations: state.conversations.map((c) => {
            if (c.id !== conversationId) return c;
            const messages = [...c.messages, newMessage];
            const tokens = {
              input: c.tokens.input + (message.tokens?.input || 0),
              output: c.tokens.output + (message.tokens?.output || 0),
            };
            return {
              ...c,
              messages,
              title: c.messages.length === 0 ? generateTitle(messages) : c.title,
              updatedAt: new Date(),
              tokens,
              cost: c.cost + (message.cost || 0),
            };
          }),
        }));
      },

      updateMessage: (conversationId, messageId, updates) => {
        set((state) => ({
          conversations: state.conversations.map((c) =>
            c.id === conversationId
              ? {
                  ...c,
                  messages: c.messages.map((m) =>
                    m.id === messageId ? { ...m, ...updates } : m
                  ),
                  updatedAt: new Date(),
                }
              : c
          ),
        }));
      },

      deleteMessage: (conversationId, messageId) => {
        set((state) => ({
          conversations: state.conversations.map((c) =>
            c.id === conversationId
              ? {
                  ...c,
                  messages: c.messages.filter((m) => m.id !== messageId),
                  updatedAt: new Date(),
                }
              : c
          ),
        }));
      },

      // Streaming Actions
      setStreaming: (isStreaming) => {
        set({ isStreaming });
      },

      addPendingToolCall: (toolCall) => {
        set((state) => ({
          pendingToolCalls: [...state.pendingToolCalls, toolCall],
        }));
      },

      updateToolCall: (id, updates) => {
        set((state) => ({
          pendingToolCalls: state.pendingToolCalls.map((tc) =>
            tc.id === id ? { ...tc, ...updates } : tc
          ),
        }));
      },

      clearPendingToolCalls: () => {
        set({ pendingToolCalls: [] });
      },

      // Settings Actions
      updateSettings: (newSettings) => {
        set((state) => ({
          settings: { ...state.settings, ...newSettings },
        }));
      },

      resetSettings: () => {
        set({ settings: DEFAULT_SETTINGS });
      },

      // UI Actions
      toggleSidebar: () => {
        set((state) => ({ sidebarOpen: !state.sidebarOpen }));
      },

      toggleSettings: () => {
        set((state) => ({ settingsOpen: !state.settingsOpen }));
      },

      toggleShortcuts: () => {
        set((state) => ({ shortcutsOpen: !state.shortcutsOpen }));
      },

      toggleExportDialog: () => {
        set((state) => ({ exportDialogOpen: !state.exportDialogOpen }));
      },

      // Connection Actions
      setConnected: (connected) => {
        set({ isConnected: connected });
        if (connected) {
          set({ reconnectAttempts: 0 });
        }
      },

      incrementReconnectAttempts: () => {
        set((state) => ({ reconnectAttempts: state.reconnectAttempts + 1 }));
      },

      resetReconnectAttempts: () => {
        set({ reconnectAttempts: 0 });
      },

      // Budget Actions
      addCost: (cost) => {
        set((state) => {
          const today = new Date().toISOString().split('T')[0];
          const thisMonth = new Date().toISOString().slice(0, 7);
          const { budgetUsage } = state;

          // Check if we need to reset periods
          let newUsage = { ...budgetUsage };
          
          if (budgetUsage.lastReset.daily !== today) {
            newUsage = { ...newUsage, daily: 0, lastReset: { ...newUsage.lastReset, daily: today } };
          }
          
          if (budgetUsage.lastReset.monthly !== thisMonth) {
            newUsage = {
              ...newUsage,
              monthly: 0,
              weekly: 0,
              lastReset: {
                ...newUsage.lastReset,
                monthly: thisMonth,
                weekly: today,
              },
            };
          }

          return {
            budgetUsage: {
              ...newUsage,
              daily: newUsage.daily + cost,
              weekly: newUsage.weekly + cost,
              monthly: newUsage.monthly + cost,
            },
          };
        });
      },

      resetBudget: (period) => {
        set((state) => ({
          budgetUsage: {
            ...state.budgetUsage,
            [period]: 0,
          },
        }));
      },

      // Computed
      getCurrentConversation: () => {
        const { conversations, currentConversationId } = get();
        return conversations.find((c) => c.id === currentConversationId);
      },

      getTotalCost: () => {
        const { conversations } = get();
        return conversations.reduce((sum, c) => sum + c.cost, 0);
      },

      getTotalTokens: () => {
        const { conversations } = get();
        return conversations.reduce(
          (sum, c) => ({
            input: sum.input + c.tokens.input,
            output: sum.output + c.tokens.output,
          }),
          { input: 0, output: 0 }
        );
      },
    }),
    {
      name: 'ember-storage',
      storage: createJSONStorage(() => localStorage),
      partialize: (state) => ({
        conversations: state.settings.saveHistory ? state.conversations : [],
        settings: state.settings,
        budgetUsage: state.budgetUsage,
      }),
    }
  )
);

// Selectors
export const selectCurrentConversation = (state: AppState) =>
  state.conversations.find((c) => c.id === state.currentConversationId);

export const selectRecentConversations = (state: AppState, limit: number = 10) =>
  state.conversations.slice(0, limit);

export const selectConversationsByDate = (state: AppState) => {
  const today = new Date();
  const yesterday = new Date(today);
  yesterday.setDate(yesterday.getDate() - 1);
  const weekAgo = new Date(today);
  weekAgo.setDate(weekAgo.getDate() - 7);

  return {
    today: state.conversations.filter(
      (c) => new Date(c.updatedAt).toDateString() === today.toDateString()
    ),
    yesterday: state.conversations.filter(
      (c) => new Date(c.updatedAt).toDateString() === yesterday.toDateString()
    ),
    thisWeek: state.conversations.filter((c) => {
      const date = new Date(c.updatedAt);
      return (
        date > weekAgo &&
        date.toDateString() !== today.toDateString() &&
        date.toDateString() !== yesterday.toDateString()
      );
    }),
    older: state.conversations.filter((c) => new Date(c.updatedAt) <= weekAgo),
  };
};

export const selectIsBudgetExceeded = (state: AppState) => {
  const { settings, budgetUsage } = state;
  if (!settings.budgetLimit) return false;
  return budgetUsage[settings.budgetPeriod] >= settings.budgetLimit;
};

export default useStore;