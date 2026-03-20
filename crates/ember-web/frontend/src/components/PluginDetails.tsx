import React, { useState } from 'react';
import { PluginMetadata } from './PluginCard';

interface PluginDetailsProps {
  plugin: PluginMetadata;
  onBack: () => void;
  onInstall: (version?: string) => void;
}

export function PluginDetails({ plugin, onBack, onInstall }: PluginDetailsProps) {
  const [selectedVersion, setSelectedVersion] = useState(plugin.versions[0]?.version || '');
  const [activeTab, setActiveTab] = useState<'readme' | 'versions' | 'reviews'>('readme');
  const [installing, setInstalling] = useState(false);

  async function handleInstall() {
    setInstalling(true);
    try {
      await onInstall(selectedVersion);
    } finally {
      setInstalling(false);
    }
  }

  function formatDate(dateStr: string): string {
    return new Date(dateStr).toLocaleDateString('en-US', {
      year: 'numeric',
      month: 'short',
      day: 'numeric',
    });
  }

  function formatDownloads(count: number): string {
    if (count >= 1000000) return `${(count / 1000000).toFixed(1)}M`;
    if (count >= 1000) return `${(count / 1000).toFixed(1)}K`;
    return count.toString();
  }

  function renderStars(rating: number): React.ReactNode {
    const stars = [];
    for (let i = 1; i <= 5; i++) {
      stars.push(
        <svg
          key={i}
          className={`star ${i <= rating ? 'filled' : ''}`}
          viewBox="0 0 24 24"
          fill={i <= rating ? 'currentColor' : 'none'}
          stroke="currentColor"
          strokeWidth="2"
        >
          <path d="M12 2l3.09 6.26L22 9.27l-5 4.87 1.18 6.88L12 17.77l-6.18 3.25L7 14.14 2 9.27l6.91-1.01L12 2z" />
        </svg>
      );
    }
    return stars;
  }

  return (
    <div className="plugin-details">
      {/* Back Button */}
      <button className="back-button" onClick={onBack} aria-label="Go back">
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
          <path d="M19 12H5M12 19l-7-7 7-7" />
        </svg>
        Back to Marketplace
      </button>

      {/* Header */}
      <div className="details-header">
        <div className="plugin-icon">
          {plugin.icon ? (
            <img src={plugin.icon} alt="" />
          ) : (
            <div className="icon-placeholder">
              {plugin.name.charAt(0).toUpperCase()}
            </div>
          )}
        </div>
        <div className="plugin-info">
          <h1>
            {plugin.name}
            {plugin.verified && (
              <span className="verified-badge" title="Verified plugin">
                <svg viewBox="0 0 24 24" fill="currentColor">
                  <path d="M9 16.17L4.83 12l-1.42 1.41L9 19 21 7l-1.41-1.41L9 16.17z" />
                </svg>
              </span>
            )}
          </h1>
          <p className="description">{plugin.description}</p>
          <div className="meta-row">
            <span className="category">{plugin.category}</span>
            <span className="license">{plugin.license}</span>
            <span className="author">
              by {plugin.authors[0]?.name || 'Unknown'}
            </span>
          </div>
        </div>
        <div className="install-section">
          <select
            value={selectedVersion}
            onChange={(e) => setSelectedVersion(e.target.value)}
            className="version-select"
            aria-label="Select version"
          >
            {plugin.versions.map((v) => (
              <option key={v.version} value={v.version}>
                v{v.version}
              </option>
            ))}
          </select>
          <button
            className="install-button"
            onClick={handleInstall}
            disabled={installing}
          >
            {installing ? 'Installing...' : 'Install'}
          </button>
        </div>
      </div>

      {/* Stats */}
      <div className="stats-bar">
        <div className="stat">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
            <path d="M21 15v4a2 2 0 01-2 2H5a2 2 0 01-2-2v-4M7 10l5 5 5-5M12 15V3" />
          </svg>
          <div>
            <strong>{formatDownloads(plugin.stats.downloads)}</strong>
            <span>Downloads</span>
          </div>
        </div>
        <div className="stat">
          <div className="stars-display">
            {renderStars(Math.round(plugin.stats.averageRating))}
          </div>
          <div>
            <strong>{plugin.stats.averageRating.toFixed(1)}</strong>
            <span>({plugin.stats.reviewCount} reviews)</span>
          </div>
        </div>
        <div className="stat">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
            <path d="M12 2l3.09 6.26L22 9.27l-5 4.87 1.18 6.88L12 17.77l-6.18 3.25L7 14.14 2 9.27l6.91-1.01L12 2z" />
          </svg>
          <div>
            <strong>{formatDownloads(plugin.stats.stars)}</strong>
            <span>Stars</span>
          </div>
        </div>
      </div>

      {/* Tabs */}
      <div className="tabs" role="tablist">
        <button
          role="tab"
          aria-selected={activeTab === 'readme'}
          className={activeTab === 'readme' ? 'active' : ''}
          onClick={() => setActiveTab('readme')}
        >
          Readme
        </button>
        <button
          role="tab"
          aria-selected={activeTab === 'versions'}
          className={activeTab === 'versions' ? 'active' : ''}
          onClick={() => setActiveTab('versions')}
        >
          Versions ({plugin.versions.length})
        </button>
        <button
          role="tab"
          aria-selected={activeTab === 'reviews'}
          className={activeTab === 'reviews' ? 'active' : ''}
          onClick={() => setActiveTab('reviews')}
        >
          Reviews ({plugin.stats.reviewCount})
        </button>
      </div>

      {/* Tab Content */}
      <div className="tab-content" role="tabpanel">
        {activeTab === 'readme' && (
          <div className="readme-content">
            <p>Plugin documentation and usage instructions would appear here.</p>
            <h2>Installation</h2>
            <pre>
              <code>ember plugin install {plugin.id}</code>
            </pre>
            <h2>Usage</h2>
            <p>Configure the plugin in your ember.toml:</p>
            <pre>
              <code>{`[plugins.${plugin.id}]
enabled = true`}</code>
            </pre>
          </div>
        )}

        {activeTab === 'versions' && (
          <div className="versions-list">
            {plugin.versions.map((version) => (
              <div key={version.version} className="version-item">
                <div className="version-header">
                  <span className="version-number">v{version.version}</span>
                  <span className="version-date">{formatDate(version.releasedAt)}</span>
                </div>
                <button
                  className="install-version-btn"
                  onClick={() => {
                    setSelectedVersion(version.version);
                    handleInstall();
                  }}
                >
                  Install
                </button>
              </div>
            ))}
          </div>
        )}

        {activeTab === 'reviews' && (
          <div className="reviews-list">
            <p className="no-reviews">No reviews yet. Be the first to review this plugin!</p>
          </div>
        )}
      </div>

      {/* Tags */}
      <div className="tags-section">
        <h3>Tags</h3>
        <div className="tags">
          {plugin.tags.map((tag) => (
            <span key={tag} className="tag">
              {tag}
            </span>
          ))}
        </div>
      </div>

      <style>{`
        .plugin-details {
          max-width: 1000px;
          margin: 0 auto;
          padding: 24px;
        }

        .back-button {
          display: flex;
          align-items: center;
          gap: 8px;
          background: none;
          border: none;
          color: var(--primary-color);
          font-size: 0.95rem;
          cursor: pointer;
          padding: 8px 0;
          margin-bottom: 24px;
        }

        .back-button svg {
          width: 20px;
          height: 20px;
        }

        .back-button:hover {
          text-decoration: underline;
        }

        .details-header {
          display: grid;
          grid-template-columns: auto 1fr auto;
          gap: 24px;
          align-items: start;
          margin-bottom: 24px;
        }

        .plugin-icon {
          width: 80px;
          height: 80px;
          border-radius: 16px;
          overflow: hidden;
        }

        .plugin-icon img {
          width: 100%;
          height: 100%;
          object-fit: cover;
        }

        .icon-placeholder {
          width: 100%;
          height: 100%;
          display: flex;
          align-items: center;
          justify-content: center;
          background: var(--primary-color);
          color: white;
          font-size: 2.5rem;
          font-weight: 600;
        }

        .plugin-info h1 {
          margin: 0;
          font-size: 1.75rem;
          display: flex;
          align-items: center;
          gap: 8px;
        }

        .verified-badge {
          color: var(--success-color);
          display: inline-flex;
        }

        .verified-badge svg {
          width: 24px;
          height: 24px;
        }

        .description {
          margin: 8px 0 12px;
          color: var(--text-secondary);
          font-size: 1.05rem;
        }

        .meta-row {
          display: flex;
          gap: 16px;
          font-size: 0.9rem;
          color: var(--text-secondary);
        }

        .category {
          padding: 4px 10px;
          background: var(--primary-color-alpha);
          color: var(--primary-color);
          border-radius: 4px;
          font-weight: 500;
        }

        .install-section {
          display: flex;
          flex-direction: column;
          gap: 12px;
        }

        .version-select {
          padding: 10px 12px;
          border: 1px solid var(--border-color);
          border-radius: 8px;
          background: var(--bg-secondary);
          color: var(--text-primary);
          font-size: 0.95rem;
        }

        .install-button {
          padding: 12px 32px;
          background: var(--primary-color);
          color: white;
          border: none;
          border-radius: 8px;
          font-weight: 600;
          font-size: 1rem;
          cursor: pointer;
          transition: background 0.2s;
        }

        .install-button:hover:not(:disabled) {
          background: var(--primary-color-dark);
        }

        .install-button:disabled {
          opacity: 0.7;
          cursor: not-allowed;
        }

        .stats-bar {
          display: flex;
          gap: 32px;
          padding: 20px 24px;
          background: var(--bg-secondary);
          border-radius: 12px;
          margin-bottom: 24px;
        }

        .stat {
          display: flex;
          align-items: center;
          gap: 12px;
        }

        .stat > svg {
          width: 24px;
          height: 24px;
          color: var(--text-secondary);
        }

        .stat div {
          display: flex;
          flex-direction: column;
        }

        .stat strong {
          font-size: 1.1rem;
        }

        .stat span {
          font-size: 0.85rem;
          color: var(--text-secondary);
        }

        .stars-display {
          display: flex;
          gap: 2px;
        }

        .star {
          width: 18px;
          height: 18px;
          color: var(--text-secondary);
        }

        .star.filled {
          color: #f59e0b;
        }

        .tabs {
          display: flex;
          gap: 4px;
          border-bottom: 1px solid var(--border-color);
          margin-bottom: 24px;
        }

        .tabs button {
          padding: 12px 20px;
          background: none;
          border: none;
          border-bottom: 2px solid transparent;
          color: var(--text-secondary);
          font-size: 0.95rem;
          cursor: pointer;
          transition: all 0.2s;
        }

        .tabs button:hover {
          color: var(--text-primary);
        }

        .tabs button.active {
          color: var(--primary-color);
          border-bottom-color: var(--primary-color);
        }

        .tab-content {
          min-height: 300px;
        }

        .readme-content {
          line-height: 1.7;
        }

        .readme-content h2 {
          margin-top: 24px;
          margin-bottom: 12px;
        }

        .readme-content pre {
          background: var(--bg-tertiary);
          padding: 16px;
          border-radius: 8px;
          overflow-x: auto;
        }

        .readme-content code {
          font-family: 'Fira Code', monospace;
          font-size: 0.9rem;
        }

        .versions-list {
          display: flex;
          flex-direction: column;
          gap: 12px;
        }

        .version-item {
          display: flex;
          justify-content: space-between;
          align-items: center;
          padding: 16px;
          background: var(--bg-secondary);
          border-radius: 8px;
        }

        .version-header {
          display: flex;
          flex-direction: column;
          gap: 4px;
        }

        .version-number {
          font-weight: 600;
        }

        .version-date {
          font-size: 0.85rem;
          color: var(--text-secondary);
        }

        .install-version-btn {
          padding: 8px 16px;
          background: var(--bg-tertiary);
          border: 1px solid var(--border-color);
          border-radius: 6px;
          color: var(--text-primary);
          cursor: pointer;
        }

        .install-version-btn:hover {
          background: var(--bg-secondary);
          border-color: var(--primary-color);
        }

        .reviews-list {
          padding: 24px;
          text-align: center;
        }

        .no-reviews {
          color: var(--text-secondary);
        }

        .tags-section {
          margin-top: 32px;
          padding-top: 24px;
          border-top: 1px solid var(--border-color);
        }

        .tags-section h3 {
          margin: 0 0 12px 0;
        }

        .tags {
          display: flex;
          flex-wrap: wrap;
          gap: 8px;
        }

        .tag {
          padding: 6px 12px;
          background: var(--bg-secondary);
          border-radius: 6px;
          font-size: 0.9rem;
          color: var(--text-secondary);
        }

        @media (max-width: 768px) {
          .details-header {
            grid-template-columns: 1fr;
          }

          .install-section {
            flex-direction: row;
          }

          .stats-bar {
            flex-wrap: wrap;
          }
        }
      `}</style>
    </div>
  );
}