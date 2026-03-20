import React, { useEffect, useState } from 'react';
import { PluginCard, PluginMetadata } from './PluginCard';
import { PluginDetails } from './PluginDetails';

interface SearchQuery {
  query?: string;
  category?: string;
  sortBy?: 'relevance' | 'downloads' | 'rating' | 'recently_updated' | 'newest';
  verifiedOnly?: boolean;
}

interface SearchResults {
  plugins: PluginMetadata[];
  total: number;
  page: number;
  pageSize: number;
  totalPages: number;
}

const CATEGORIES = [
  { id: 'all', name: 'All Categories' },
  { id: 'integration', name: 'Integration' },
  { id: 'ai', name: 'AI' },
  { id: 'developer', name: 'Developer' },
  { id: 'productivity', name: 'Productivity' },
  { id: 'data', name: 'Data' },
  { id: 'security', name: 'Security' },
  { id: 'communication', name: 'Communication' },
  { id: 'utility', name: 'Utility' },
];

const SORT_OPTIONS = [
  { id: 'relevance', name: 'Relevance' },
  { id: 'downloads', name: 'Most Downloads' },
  { id: 'rating', name: 'Highest Rated' },
  { id: 'recently_updated', name: 'Recently Updated' },
  { id: 'newest', name: 'Newest' },
];

export function PluginMarketplace() {
  const [searchQuery, setSearchQuery] = useState('');
  const [category, setCategory] = useState('all');
  const [sortBy, setSortBy] = useState<SearchQuery['sortBy']>('relevance');
  const [verifiedOnly, setVerifiedOnly] = useState(false);
  const [results, setResults] = useState<SearchResults | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [selectedPlugin, setSelectedPlugin] = useState<PluginMetadata | null>(null);
  const [featuredPlugins, setFeaturedPlugins] = useState<PluginMetadata[]>([]);
  const [page, setPage] = useState(1);

  // Load featured plugins on mount
  useEffect(() => {
    loadFeaturedPlugins();
  }, []);

  // Search when filters change
  useEffect(() => {
    if (searchQuery || category !== 'all') {
      searchPlugins();
    }
  }, [searchQuery, category, sortBy, verifiedOnly, page]);

  async function loadFeaturedPlugins() {
    try {
      const response = await fetch('/api/plugins/featured');
      if (response.ok) {
        const data = await response.json();
        setFeaturedPlugins(data);
      }
    } catch (err) {
      console.error('Failed to load featured plugins:', err);
    }
  }

  async function searchPlugins() {
    setLoading(true);
    setError(null);

    try {
      const params = new URLSearchParams();
      if (searchQuery) params.set('query', searchQuery);
      if (category !== 'all') params.set('category', category);
      if (sortBy) params.set('sort_by', sortBy);
      if (verifiedOnly) params.set('verified_only', 'true');
      params.set('page', page.toString());
      params.set('page_size', '20');

      const response = await fetch(`/api/plugins/search?${params}`);
      if (!response.ok) {
        throw new Error('Search failed');
      }

      const data: SearchResults = await response.json();
      setResults(data);
    } catch (err) {
      setError(err instanceof Error ? err.message : 'An error occurred');
    } finally {
      setLoading(false);
    }
  }

  async function installPlugin(pluginId: string, version?: string) {
    try {
      const response = await fetch('/api/plugins/install', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ plugin_id: pluginId, version }),
      });

      if (!response.ok) {
        throw new Error('Installation failed');
      }

      // Refresh plugin data
      if (selectedPlugin) {
        const updated = await fetch(`/api/plugins/${pluginId}`);
        if (updated.ok) {
          setSelectedPlugin(await updated.json());
        }
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Installation failed');
    }
  }

  function handleSearch(e: React.FormEvent) {
    e.preventDefault();
    setPage(1);
    searchPlugins();
  }

  if (selectedPlugin) {
    return (
      <PluginDetails
        plugin={selectedPlugin}
        onBack={() => setSelectedPlugin(null)}
        onInstall={(version) => installPlugin(selectedPlugin.id, version)}
      />
    );
  }

  return (
    <div className="plugin-marketplace">
      {/* Header */}
      <div className="marketplace-header">
        <h1>Plugin Marketplace</h1>
        <p className="subtitle">Extend Ember with powerful plugins</p>
      </div>

      {/* Search and Filters */}
      <div className="marketplace-filters">
        <form onSubmit={handleSearch} className="search-form">
          <div className="search-input-wrapper">
            <svg
              className="search-icon"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              strokeWidth="2"
            >
              <circle cx="11" cy="11" r="8" />
              <path d="M21 21l-4.35-4.35" />
            </svg>
            <input
              type="text"
              value={searchQuery}
              onChange={(e) => setSearchQuery(e.target.value)}
              placeholder="Search plugins..."
              className="search-input"
              aria-label="Search plugins"
            />
          </div>
          <button type="submit" className="search-button">
            Search
          </button>
        </form>

        <div className="filter-row">
          <select
            value={category}
            onChange={(e) => {
              setCategory(e.target.value);
              setPage(1);
            }}
            className="filter-select"
            aria-label="Filter by category"
          >
            {CATEGORIES.map((cat) => (
              <option key={cat.id} value={cat.id}>
                {cat.name}
              </option>
            ))}
          </select>

          <select
            value={sortBy}
            onChange={(e) => {
              setSortBy(e.target.value as SearchQuery['sortBy']);
              setPage(1);
            }}
            className="filter-select"
            aria-label="Sort by"
          >
            {SORT_OPTIONS.map((opt) => (
              <option key={opt.id} value={opt.id}>
                {opt.name}
              </option>
            ))}
          </select>

          <label className="checkbox-label">
            <input
              type="checkbox"
              checked={verifiedOnly}
              onChange={(e) => {
                setVerifiedOnly(e.target.checked);
                setPage(1);
              }}
            />
            <span>Verified only</span>
          </label>
        </div>
      </div>

      {/* Error Message */}
      {error && (
        <div className="error-banner" role="alert">
          <span>{error}</span>
          <button onClick={() => setError(null)} aria-label="Dismiss error">
            x
          </button>
        </div>
      )}

      {/* Loading State */}
      {loading && (
        <div className="loading-container">
          <div className="loading-spinner" />
          <span>Searching plugins...</span>
        </div>
      )}

      {/* Results */}
      {!loading && results && (
        <div className="search-results">
          <div className="results-header">
            <span>{results.total} plugins found</span>
          </div>
          <div className="plugin-grid">
            {results.plugins.map((plugin) => (
              <PluginCard
                key={plugin.id}
                plugin={plugin}
                onClick={() => setSelectedPlugin(plugin)}
                onInstall={() => installPlugin(plugin.id)}
              />
            ))}
          </div>

          {/* Pagination */}
          {results.totalPages > 1 && (
            <div className="pagination">
              <button
                onClick={() => setPage((p) => Math.max(1, p - 1))}
                disabled={page === 1}
                aria-label="Previous page"
              >
                Previous
              </button>
              <span>
                Page {page} of {results.totalPages}
              </span>
              <button
                onClick={() => setPage((p) => Math.min(results.totalPages, p + 1))}
                disabled={page === results.totalPages}
                aria-label="Next page"
              >
                Next
              </button>
            </div>
          )}
        </div>
      )}

      {/* Featured Plugins (when no search) */}
      {!loading && !results && featuredPlugins.length > 0 && (
        <div className="featured-section">
          <h2>Featured Plugins</h2>
          <div className="plugin-grid">
            {featuredPlugins.map((plugin) => (
              <PluginCard
                key={plugin.id}
                plugin={plugin}
                onClick={() => setSelectedPlugin(plugin)}
                onInstall={() => installPlugin(plugin.id)}
                featured
              />
            ))}
          </div>
        </div>
      )}

      {/* Empty State */}
      {!loading && results && results.plugins.length === 0 && (
        <div className="empty-state">
          <svg
            className="empty-icon"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            strokeWidth="1.5"
          >
            <path d="M20 7l-8-4-8 4m16 0l-8 4m8-4v10l-8 4m0-10L4 7m8 4v10M4 7v10l8 4" />
          </svg>
          <h3>No plugins found</h3>
          <p>Try adjusting your search or filters</p>
        </div>
      )}

      <style>{`
        .plugin-marketplace {
          padding: 24px;
          max-width: 1200px;
          margin: 0 auto;
        }

        .marketplace-header {
          text-align: center;
          margin-bottom: 32px;
        }

        .marketplace-header h1 {
          font-size: 2rem;
          font-weight: 700;
          margin: 0;
        }

        .subtitle {
          color: var(--text-secondary);
          margin-top: 8px;
        }

        .marketplace-filters {
          margin-bottom: 24px;
        }

        .search-form {
          display: flex;
          gap: 12px;
          margin-bottom: 16px;
        }

        .search-input-wrapper {
          flex: 1;
          position: relative;
        }

        .search-icon {
          position: absolute;
          left: 12px;
          top: 50%;
          transform: translateY(-50%);
          width: 20px;
          height: 20px;
          color: var(--text-secondary);
        }

        .search-input {
          width: 100%;
          padding: 12px 12px 12px 44px;
          border: 1px solid var(--border-color);
          border-radius: 8px;
          font-size: 1rem;
          background: var(--bg-secondary);
          color: var(--text-primary);
        }

        .search-input:focus {
          outline: none;
          border-color: var(--primary-color);
          box-shadow: 0 0 0 3px var(--primary-color-alpha);
        }

        .search-button {
          padding: 12px 24px;
          background: var(--primary-color);
          color: white;
          border: none;
          border-radius: 8px;
          font-weight: 600;
          cursor: pointer;
          transition: background 0.2s;
        }

        .search-button:hover {
          background: var(--primary-color-dark);
        }

        .filter-row {
          display: flex;
          gap: 12px;
          align-items: center;
          flex-wrap: wrap;
        }

        .filter-select {
          padding: 8px 12px;
          border: 1px solid var(--border-color);
          border-radius: 6px;
          background: var(--bg-secondary);
          color: var(--text-primary);
          font-size: 0.9rem;
        }

        .checkbox-label {
          display: flex;
          align-items: center;
          gap: 8px;
          cursor: pointer;
        }

        .checkbox-label input {
          width: 16px;
          height: 16px;
        }

        .error-banner {
          display: flex;
          justify-content: space-between;
          align-items: center;
          padding: 12px 16px;
          background: var(--error-bg);
          color: var(--error-color);
          border-radius: 8px;
          margin-bottom: 24px;
        }

        .error-banner button {
          background: none;
          border: none;
          color: inherit;
          cursor: pointer;
          font-size: 1.2rem;
        }

        .loading-container {
          display: flex;
          flex-direction: column;
          align-items: center;
          padding: 48px;
          gap: 16px;
        }

        .loading-spinner {
          width: 40px;
          height: 40px;
          border: 3px solid var(--border-color);
          border-top-color: var(--primary-color);
          border-radius: 50%;
          animation: spin 1s linear infinite;
        }

        @keyframes spin {
          to { transform: rotate(360deg); }
        }

        .results-header {
          margin-bottom: 16px;
          color: var(--text-secondary);
        }

        .plugin-grid {
          display: grid;
          grid-template-columns: repeat(auto-fill, minmax(300px, 1fr));
          gap: 20px;
        }

        .pagination {
          display: flex;
          justify-content: center;
          align-items: center;
          gap: 16px;
          margin-top: 32px;
        }

        .pagination button {
          padding: 8px 16px;
          border: 1px solid var(--border-color);
          border-radius: 6px;
          background: var(--bg-secondary);
          color: var(--text-primary);
          cursor: pointer;
        }

        .pagination button:disabled {
          opacity: 0.5;
          cursor: not-allowed;
        }

        .pagination button:not(:disabled):hover {
          background: var(--bg-tertiary);
        }

        .featured-section h2 {
          margin-bottom: 20px;
        }

        .empty-state {
          text-align: center;
          padding: 48px;
        }

        .empty-icon {
          width: 64px;
          height: 64px;
          color: var(--text-secondary);
          margin-bottom: 16px;
        }

        .empty-state h3 {
          margin: 0 0 8px 0;
        }

        .empty-state p {
          color: var(--text-secondary);
          margin: 0;
        }
      `}</style>
    </div>
  );
}