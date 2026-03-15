import {
    Activity, Bot, Boxes,
    ChevronRight,
    Flame,
    Globe,
    LayoutDashboard,
    MessageSquare, Network, Play, RefreshCw, Settings, Shield,
    Sparkles,
    Terminal, TrendingUp, Users, Wrench, Zap
} from 'lucide-react'
import { useCallback, useState } from 'react'

// Types
interface SystemMetrics {
  cpu_usage: number
  memory_usage: number
  active_agents: number
  active_tasks: number
  messages_per_minute: number
  uptime_seconds: number
}

interface AgentInfo {
  id: string
  name: string
  role: string
  status: 'idle' | 'working' | 'waiting' | 'error'
  current_task?: string
  workload: number
  capabilities: string[]
}

interface TaskInfo {
  id: string
  description: string
  status: 'pending' | 'assigned' | 'in_progress' | 'completed' | 'failed'
  assignee?: string
  progress: number
  priority: number
  created_at: string
}

interface ToolInfo {
  name: string
  description: string
  enabled: boolean
  usage_count: number
  last_used?: string
}

interface PluginInfo {
  name: string
  version: string
  description: string
  enabled: boolean
  author: string
}

// Mock data for demonstration
const mockMetrics: SystemMetrics = {
  cpu_usage: 23.5,
  memory_usage: 45.2,
  active_agents: 3,
  active_tasks: 7,
  messages_per_minute: 12,
  uptime_seconds: 3600 * 24 * 3 + 3600 * 5 + 60 * 23
}

const mockAgents: AgentInfo[] = [
  { id: 'agent-1', name: 'Coder', role: 'Coder', status: 'working', current_task: 'Implementing feature X', workload: 2, capabilities: ['coding', 'debugging', 'testing'] },
  { id: 'agent-2', name: 'Researcher', role: 'Researcher', status: 'idle', workload: 0, capabilities: ['search', 'analysis', 'summarization'] },
  { id: 'agent-3', name: 'Reviewer', role: 'Reviewer', status: 'waiting', current_task: 'Waiting for code review', workload: 1, capabilities: ['review', 'quality'] },
]

const mockTasks: TaskInfo[] = [
  { id: 'task-1', description: 'Implement user authentication', status: 'in_progress', assignee: 'agent-1', progress: 65, priority: 8, created_at: '2024-01-15T10:30:00Z' },
  { id: 'task-2', description: 'Research best practices for caching', status: 'pending', progress: 0, priority: 5, created_at: '2024-01-15T11:00:00Z' },
  { id: 'task-3', description: 'Review PR #42', status: 'assigned', assignee: 'agent-3', progress: 20, priority: 7, created_at: '2024-01-15T09:00:00Z' },
  { id: 'task-4', description: 'Write unit tests for API', status: 'completed', assignee: 'agent-1', progress: 100, priority: 6, created_at: '2024-01-14T15:00:00Z' },
]

const mockTools: ToolInfo[] = [
  { name: 'shell', description: 'Execute shell commands', enabled: true, usage_count: 156, last_used: '2024-01-15T10:45:00Z' },
  { name: 'filesystem', description: 'Read, write, and manage files', enabled: true, usage_count: 423, last_used: '2024-01-15T10:50:00Z' },
  { name: 'web', description: 'Make HTTP requests', enabled: true, usage_count: 89, last_used: '2024-01-15T09:30:00Z' },
  { name: 'git', description: 'Git version control operations', enabled: true, usage_count: 67, last_used: '2024-01-15T10:20:00Z' },
  { name: 'browser', description: 'Headless browser automation', enabled: false, usage_count: 12 },
]

const mockPlugins: PluginInfo[] = [
  { name: 'code-analyzer', version: '1.2.0', description: 'Advanced code analysis and suggestions', enabled: true, author: 'Ember Team' },
  { name: 'doc-generator', version: '0.9.1', description: 'Automatic documentation generation', enabled: true, author: 'Community' },
  { name: 'test-runner', version: '2.0.0', description: 'Intelligent test discovery and execution', enabled: false, author: 'Ember Team' },
]

// Utility functions
function formatUptime(seconds: number): string {
  const days = Math.floor(seconds / 86400)
  const hours = Math.floor((seconds % 86400) / 3600)
  const minutes = Math.floor((seconds % 3600) / 60)
  return `${days}d ${hours}h ${minutes}m`
}

function formatDate(dateStr: string): string {
  const date = new Date(dateStr)
  return date.toLocaleString()
}

// Components
function MetricCard({ 
  icon: Icon, 
  label, 
  value, 
  unit,
  trend,
  color = 'orange'
}: { 
  icon: any
  label: string
  value: string | number
  unit?: string
  trend?: 'up' | 'down' | 'stable'
  color?: string
}) {
  const colorClasses = {
    orange: 'text-orange-500 bg-orange-500/10',
    green: 'text-green-500 bg-green-500/10',
    blue: 'text-blue-500 bg-blue-500/10',
    purple: 'text-purple-500 bg-purple-500/10',
    red: 'text-red-500 bg-red-500/10',
  }

  return (
    <div className="bg-gray-800 rounded-xl p-4 border border-gray-700">
      <div className="flex items-center justify-between mb-3">
        <div className={`p-2 rounded-lg ${colorClasses[color as keyof typeof colorClasses]}`}>
          <Icon className="w-5 h-5" />
        </div>
        {trend && (
          <div className={`flex items-center text-sm ${
            trend === 'up' ? 'text-green-400' : trend === 'down' ? 'text-red-400' : 'text-gray-400'
          }`}>
            <TrendingUp className={`w-4 h-4 mr-1 ${trend === 'down' ? 'rotate-180' : ''}`} />
            {trend === 'up' ? '+5%' : trend === 'down' ? '-3%' : '0%'}
          </div>
        )}
      </div>
      <p className="text-2xl font-bold text-white">
        {value}
        {unit && <span className="text-sm font-normal text-gray-400 ml-1">{unit}</span>}
      </p>
      <p className="text-sm text-gray-400 mt-1">{label}</p>
    </div>
  )
}

function AgentCard({ agent }: { agent: AgentInfo }) {
  const statusColors = {
    idle: 'bg-gray-500',
    working: 'bg-green-500',
    waiting: 'bg-yellow-500',
    error: 'bg-red-500',
  }

  const roleIcons: Record<string, any> = {
    Coder: Terminal,
    Researcher: Globe,
    Reviewer: Shield,
    Architect: Network,
    Tester: Wrench,
    default: Bot,
  }

  const RoleIcon = roleIcons[agent.role] || roleIcons.default

  return (
    <div className="bg-gray-800 rounded-xl p-4 border border-gray-700 hover:border-orange-500/50 transition-colors">
      <div className="flex items-start justify-between mb-3">
        <div className="flex items-center gap-3">
          <div className="p-2 rounded-lg bg-orange-500/10">
            <RoleIcon className="w-5 h-5 text-orange-500" />
          </div>
          <div>
            <h3 className="font-semibold text-white">{agent.name}</h3>
            <p className="text-sm text-gray-400">{agent.role}</p>
          </div>
        </div>
        <div className="flex items-center gap-2">
          <span className={`w-2 h-2 rounded-full ${statusColors[agent.status]}`} />
          <span className="text-sm text-gray-400 capitalize">{agent.status}</span>
        </div>
      </div>
      
      {agent.current_task && (
        <div className="mb-3 p-2 bg-gray-700/50 rounded-lg">
          <p className="text-sm text-gray-300 truncate">{agent.current_task}</p>
        </div>
      )}
      
      <div className="flex items-center justify-between text-sm">
        <span className="text-gray-400">Workload: {agent.workload} tasks</span>
        <div className="flex gap-1">
          {agent.capabilities.slice(0, 3).map((cap, i) => (
            <span key={i} className="px-2 py-0.5 bg-gray-700 text-gray-300 rounded text-xs">
              {cap}
            </span>
          ))}
        </div>
      </div>
    </div>
  )
}

function TaskRow({ task }: { task: TaskInfo }) {
  const statusColors = {
    pending: 'text-gray-400 bg-gray-500/10',
    assigned: 'text-blue-400 bg-blue-500/10',
    in_progress: 'text-yellow-400 bg-yellow-500/10',
    completed: 'text-green-400 bg-green-500/10',
    failed: 'text-red-400 bg-red-500/10',
  }

  return (
    <tr className="border-b border-gray-700 hover:bg-gray-800/50">
      <td className="py-3 px-4">
        <div className="flex items-center gap-3">
          <div className="w-8 h-8 rounded-lg bg-gray-700 flex items-center justify-center">
            <Activity className="w-4 h-4 text-gray-400" />
          </div>
          <div>
            <p className="font-medium text-white truncate max-w-xs">{task.description}</p>
            <p className="text-xs text-gray-500">{task.id}</p>
          </div>
        </div>
      </td>
      <td className="py-3 px-4">
        <span className={`px-2 py-1 rounded-full text-xs ${statusColors[task.status]}`}>
          {task.status.replace('_', ' ')}
        </span>
      </td>
      <td className="py-3 px-4">
        <div className="w-24">
          <div className="flex items-center justify-between text-xs mb-1">
            <span className="text-gray-400">{task.progress}%</span>
          </div>
          <div className="w-full bg-gray-700 rounded-full h-1.5">
            <div 
              className="bg-orange-500 h-1.5 rounded-full transition-all"
              style={{ width: `${task.progress}%` }}
            />
          </div>
        </div>
      </td>
      <td className="py-3 px-4 text-gray-400 text-sm">
        {task.assignee || '-'}
      </td>
      <td className="py-3 px-4">
        <div className="flex items-center gap-1">
          {Array.from({ length: 5 }).map((_, i) => (
            <div
              key={i}
              className={`w-1.5 h-3 rounded-sm ${
                i < Math.ceil(task.priority / 2) ? 'bg-orange-500' : 'bg-gray-600'
              }`}
            />
          ))}
        </div>
      </td>
    </tr>
  )
}

function ToolRow({ tool, onToggle }: { tool: ToolInfo; onToggle: () => void }) {
  return (
    <div className="flex items-center justify-between py-3 border-b border-gray-700 last:border-0">
      <div className="flex items-center gap-3">
        <div className={`p-2 rounded-lg ${tool.enabled ? 'bg-green-500/10' : 'bg-gray-700'}`}>
          <Wrench className={`w-4 h-4 ${tool.enabled ? 'text-green-500' : 'text-gray-500'}`} />
        </div>
        <div>
          <p className="font-medium text-white">{tool.name}</p>
          <p className="text-sm text-gray-400">{tool.description}</p>
        </div>
      </div>
      <div className="flex items-center gap-4">
        <span className="text-sm text-gray-400">{tool.usage_count} uses</span>
        <button
          onClick={onToggle}
          className={`relative w-10 h-5 rounded-full transition-colors ${
            tool.enabled ? 'bg-orange-500' : 'bg-gray-600'
          }`}
        >
          <span
            className={`absolute top-0.5 left-0.5 w-4 h-4 bg-white rounded-full transition-transform ${
              tool.enabled ? 'translate-x-5' : 'translate-x-0'
            }`}
          />
        </button>
      </div>
    </div>
  )
}

// Main Dashboard Component
export default function Dashboard() {
  const [activeTab, setActiveTab] = useState<'overview' | 'agents' | 'tasks' | 'tools' | 'plugins'>('overview')
  const [metrics, setMetrics] = useState<SystemMetrics>(mockMetrics)
  const [agents, setAgents] = useState<AgentInfo[]>(mockAgents)
  const [tasks, setTasks] = useState<TaskInfo[]>(mockTasks)
  const [tools, setTools] = useState<ToolInfo[]>(mockTools)
  const [plugins, setPlugins] = useState<PluginInfo[]>(mockPlugins)
  const [isRefreshing, setIsRefreshing] = useState(false)

  const refreshData = useCallback(async () => {
    setIsRefreshing(true)
    // In production, fetch real data from API
    await new Promise(resolve => setTimeout(resolve, 1000))
    setIsRefreshing(false)
  }, [])

  const toggleTool = (toolName: string) => {
    setTools(tools.map(t => 
      t.name === toolName ? { ...t, enabled: !t.enabled } : t
    ))
  }

  const togglePlugin = (pluginName: string) => {
    setPlugins(plugins.map(p => 
      p.name === pluginName ? { ...p, enabled: !p.enabled } : p
    ))
  }

  const tabs = [
    { id: 'overview', label: 'Overview', icon: LayoutDashboard },
    { id: 'agents', label: 'Agents', icon: Users },
    { id: 'tasks', label: 'Tasks', icon: Activity },
    { id: 'tools', label: 'Tools', icon: Wrench },
    { id: 'plugins', label: 'Plugins', icon: Boxes },
  ]

  return (
    <div className="min-h-screen bg-gray-900">
      {/* Header */}
      <header className="bg-gray-800 border-b border-gray-700 px-6 py-4">
        <div className="flex items-center justify-between">
          <div className="flex items-center gap-4">
            <Flame className="w-8 h-8 text-orange-500" />
            <div>
              <h1 className="text-xl font-bold text-white">Ember Dashboard</h1>
              <p className="text-sm text-gray-400">Multi-Agent AI System Monitor</p>
            </div>
          </div>
          <div className="flex items-center gap-3">
            <button
              onClick={refreshData}
              disabled={isRefreshing}
              className="flex items-center gap-2 px-4 py-2 bg-gray-700 hover:bg-gray-600 text-white rounded-lg transition-colors disabled:opacity-50"
            >
              <RefreshCw className={`w-4 h-4 ${isRefreshing ? 'animate-spin' : ''}`} />
              Refresh
            </button>
            <button className="p-2 bg-gray-700 hover:bg-gray-600 text-white rounded-lg transition-colors">
              <Settings className="w-5 h-5" />
            </button>
          </div>
        </div>
      </header>

      <div className="flex">
        {/* Sidebar */}
        <nav className="w-64 bg-gray-800 border-r border-gray-700 min-h-[calc(100vh-73px)]">
          <div className="p-4">
            {tabs.map(tab => (
              <button
                key={tab.id}
                onClick={() => setActiveTab(tab.id as any)}
                className={`w-full flex items-center gap-3 px-4 py-3 rounded-lg mb-1 transition-colors ${
                  activeTab === tab.id
                    ? 'bg-orange-500/10 text-orange-500'
                    : 'text-gray-400 hover:bg-gray-700 hover:text-white'
                }`}
              >
                <tab.icon className="w-5 h-5" />
                <span className="font-medium">{tab.label}</span>
                {activeTab === tab.id && <ChevronRight className="w-4 h-4 ml-auto" />}
              </button>
            ))}
          </div>
          
          {/* Quick Stats */}
          <div className="p-4 border-t border-gray-700">
            <h3 className="text-sm font-semibold text-gray-400 mb-3">System Status</h3>
            <div className="space-y-2">
              <div className="flex items-center justify-between">
                <span className="text-sm text-gray-400">CPU</span>
                <span className="text-sm text-white">{metrics.cpu_usage}%</span>
              </div>
              <div className="w-full bg-gray-700 rounded-full h-1">
                <div 
                  className="bg-orange-500 h-1 rounded-full" 
                  style={{ width: `${metrics.cpu_usage}%` }}
                />
              </div>
              <div className="flex items-center justify-between mt-3">
                <span className="text-sm text-gray-400">Memory</span>
                <span className="text-sm text-white">{metrics.memory_usage}%</span>
              </div>
              <div className="w-full bg-gray-700 rounded-full h-1">
                <div 
                  className="bg-blue-500 h-1 rounded-full" 
                  style={{ width: `${metrics.memory_usage}%` }}
                />
              </div>
            </div>
          </div>
        </nav>

        {/* Main Content */}
        <main className="flex-1 p-6">
          {activeTab === 'overview' && (
            <div className="space-y-6">
              {/* Metrics Grid */}
              <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-4 gap-4">
                <MetricCard
                  icon={Users}
                  label="Active Agents"
                  value={metrics.active_agents}
                  color="orange"
                  trend="stable"
                />
                <MetricCard
                  icon={Activity}
                  label="Active Tasks"
                  value={metrics.active_tasks}
                  color="blue"
                  trend="up"
                />
                <MetricCard
                  icon={MessageSquare}
                  label="Messages/min"
                  value={metrics.messages_per_minute}
                  color="green"
                  trend="up"
                />
                <MetricCard
                  icon={Zap}
                  label="Uptime"
                  value={formatUptime(metrics.uptime_seconds)}
                  color="purple"
                />
              </div>

              {/* Agents Overview */}
              <div className="bg-gray-800 rounded-xl border border-gray-700">
                <div className="flex items-center justify-between p-4 border-b border-gray-700">
                  <h2 className="text-lg font-semibold text-white">Agents</h2>
                  <button className="text-sm text-orange-500 hover:text-orange-400">
                    View All
                  </button>
                </div>
                <div className="p-4 grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
                  {agents.map(agent => (
                    <AgentCard key={agent.id} agent={agent} />
                  ))}
                </div>
              </div>

              {/* Recent Tasks */}
              <div className="bg-gray-800 rounded-xl border border-gray-700">
                <div className="flex items-center justify-between p-4 border-b border-gray-700">
                  <h2 className="text-lg font-semibold text-white">Recent Tasks</h2>
                  <button className="text-sm text-orange-500 hover:text-orange-400">
                    View All
                  </button>
                </div>
                <div className="overflow-x-auto">
                  <table className="w-full">
                    <thead>
                      <tr className="border-b border-gray-700">
                        <th className="text-left py-3 px-4 text-sm font-medium text-gray-400">Task</th>
                        <th className="text-left py-3 px-4 text-sm font-medium text-gray-400">Status</th>
                        <th className="text-left py-3 px-4 text-sm font-medium text-gray-400">Progress</th>
                        <th className="text-left py-3 px-4 text-sm font-medium text-gray-400">Assignee</th>
                        <th className="text-left py-3 px-4 text-sm font-medium text-gray-400">Priority</th>
                      </tr>
                    </thead>
                    <tbody>
                      {tasks.slice(0, 4).map(task => (
                        <TaskRow key={task.id} task={task} />
                      ))}
                    </tbody>
                  </table>
                </div>
              </div>
            </div>
          )}

          {activeTab === 'agents' && (
            <div className="space-y-6">
              <div className="flex items-center justify-between">
                <h2 className="text-2xl font-bold text-white">Agent Management</h2>
                <button className="flex items-center gap-2 px-4 py-2 bg-orange-500 hover:bg-orange-600 text-white rounded-lg transition-colors">
                  <Play className="w-4 h-4" />
                  Deploy Agent
                </button>
              </div>
              <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
                {agents.map(agent => (
                  <AgentCard key={agent.id} agent={agent} />
                ))}
              </div>
            </div>
          )}

          {activeTab === 'tasks' && (
            <div className="space-y-6">
              <div className="flex items-center justify-between">
                <h2 className="text-2xl font-bold text-white">Task Queue</h2>
                <button className="flex items-center gap-2 px-4 py-2 bg-orange-500 hover:bg-orange-600 text-white rounded-lg transition-colors">
                  <Sparkles className="w-4 h-4" />
                  Create Task
                </button>
              </div>
              <div className="bg-gray-800 rounded-xl border border-gray-700">
                <div className="overflow-x-auto">
                  <table className="w-full">
                    <thead>
                      <tr className="border-b border-gray-700">
                        <th className="text-left py-3 px-4 text-sm font-medium text-gray-400">Task</th>
                        <th className="text-left py-3 px-4 text-sm font-medium text-gray-400">Status</th>
                        <th className="text-left py-3 px-4 text-sm font-medium text-gray-400">Progress</th>
                        <th className="text-left py-3 px-4 text-sm font-medium text-gray-400">Assignee</th>
                        <th className="text-left py-3 px-4 text-sm font-medium text-gray-400">Priority</th>
                      </tr>
                    </thead>
                    <tbody>
                      {tasks.map(task => (
                        <TaskRow key={task.id} task={task} />
                      ))}
                    </tbody>
                  </table>
                </div>
              </div>
            </div>
          )}

          {activeTab === 'tools' && (
            <div className="space-y-6">
              <div className="flex items-center justify-between">
                <h2 className="text-2xl font-bold text-white">Tool Configuration</h2>
              </div>
              <div className="bg-gray-800 rounded-xl border border-gray-700 p-4">
                {tools.map(tool => (
                  <ToolRow 
                    key={tool.name} 
                    tool={tool} 
                    onToggle={() => toggleTool(tool.name)} 
                  />
                ))}
              </div>
            </div>
          )}

          {activeTab === 'plugins' && (
            <div className="space-y-6">
              <div className="flex items-center justify-between">
                <h2 className="text-2xl font-bold text-white">Plugin Marketplace</h2>
                <button className="flex items-center gap-2 px-4 py-2 bg-orange-500 hover:bg-orange-600 text-white rounded-lg transition-colors">
                  <Boxes className="w-4 h-4" />
                  Browse Plugins
                </button>
              </div>
              <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
                {plugins.map(plugin => (
                  <div key={plugin.name} className="bg-gray-800 rounded-xl p-4 border border-gray-700">
                    <div className="flex items-start justify-between mb-3">
                      <div className="flex items-center gap-3">
                        <div className="p-2 rounded-lg bg-purple-500/10">
                          <Boxes className="w-5 h-5 text-purple-500" />
                        </div>
                        <div>
                          <h3 className="font-semibold text-white">{plugin.name}</h3>
                          <p className="text-xs text-gray-400">v{plugin.version}</p>
                        </div>
                      </div>
                      <button
                        onClick={() => togglePlugin(plugin.name)}
                        className={`relative w-10 h-5 rounded-full transition-colors ${
                          plugin.enabled ? 'bg-orange-500' : 'bg-gray-600'
                        }`}
                      >
                        <span
                          className={`absolute top-0.5 left-0.5 w-4 h-4 bg-white rounded-full transition-transform ${
                            plugin.enabled ? 'translate-x-5' : 'translate-x-0'
                          }`}
                        />
                      </button>
                    </div>
                    <p className="text-sm text-gray-400 mb-3">{plugin.description}</p>
                    <p className="text-xs text-gray-500">by {plugin.author}</p>
                  </div>
                ))}
              </div>
            </div>
          )}
        </main>
      </div>
    </div>
  )
}