import React from 'react';

export interface PluginMetadata {
  id: string;
  name: string;
  description: string;
  category: string;
  tags: string[];
  authors: Array<{ name: string; verified: boolean }>;
  license: string;
  icon?: string;
  verified: boolean;
  featured: boolean;
  stats: {
    downloads: number;
    stars: number;
    averageRating: number;
    reviewCount: number;
  };
  versions: Array<{
    version: string;
    releasedAt: string;
  }>;
}

interface PluginCardProps {
  plugin: PluginMetadata;
  onClick: () => void;
  onInstall: () => void;
  featured?: boolean;
}

export function PluginCard({ plugin, onClick, onInstall, featured }: PluginCardProps) {
  const latestVersion = plugin.versions[0]?.version || 'N/A';
  const rating = plugin.stats.averageRating.toFixed(1);

  function formatDownloads(count: number): string {
    if (count >= 1000000) {
      return `${(count / 1000000).toFixed(1)}M`;
    }
    if (count >= 1000) {
      return `${(count / 1000).toFixed(1)}K`;
    }
    return count.toString();
  }

  function handleInstallClick(e: React.MouseEvent) {
    e.stopPropagation();
    onInstall();
  }

  return (
    <div
      className={`plugin-card ${featured ? 'featured' : ''}`}
      onClick={onClick}
      role="button"
      tabIndex={0}
      onKeyPress={(e) => e.key === 'Enter' && onClick()}
      aria-label={`View details for ${plugin.name}`}
    >
      {/* Header */}
      <div className="card-header">
        <div className="plugin-icon">
          {plugin.icon ? (
            <img src={plugin.icon} alt="" />
          ) : (
            <div className="icon-placeholder">
              {plugin.name.charAt(0).toUpperCase()}
            </div>
          )}
        </div>
        <div className="plugin-title">
          <h3>
            {plugin.name}
            {plugin.verified && (
              <span className="verified-badge" title="Verified plugin">
                <svg viewBox="0 0 24 24" fill="currentColor" width="16" height="16">
                  <path d="M9 16.17L4.83 12l-1.42 1.41L9 19 21 7l-1.41-1.41L9 16.17z" />
                </svg>
              </span>
            )}
          </h3>
          <span className="version">v{latestVersion}</span>
        </div>
      </div>

      {/* Description */}
      <p className="description">{plugin.description}</p>

      {/* Author */}
      <div className="author">
        by {plugin.authors[0]?.name || 'Unknown'}
        {plugin.authors[0]?.verified && (
          <span className="author-verified" title="Verified author">
            <svg viewBox="0 0 24 24" fill="currentColor" width="12" height="12">
              <path d="M9 16.17L4.83 12l-1.42 1.41L9 19 21 7l-1.41-1.41L9 16.17z" />
            </svg>
          </span>
        )}
      </div>

      {/* Tags */}
      <div className="tags">
        <span className="category-tag">{plugin.category}</span>
        {plugin.tags.slice(0, 2).map((tag) => (
          <span key={tag} className="tag">
            {tag}
          </span>
        ))}
      </div>

      {/* Stats */}
      <div className="stats-row">
        <div className="stat" title={`${plugin.stats.downloads} downloads`}>
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
            <path d="M21 15v4a2 2 0 01-2 2H5a2 2 0 01-2-2v-4M7 10l5 5 5-5M12 15V3" />
          </svg>
          <span>{formatDownloads(plugin.stats.downloads)}</span>
        </div>
        <div className="stat" title={`${plugin.stats.reviewCount} reviews`}>
          <svg viewBox="0 0 24 24" fill="currentColor">
            <path d="M12 2l3.09 6.26L22 9.27l-5 4.87 1.18 6.88L12 17.77l-6.18 3.25L7 14.14 2 9.27l6.91-1.01L12 2z" />
          </svg>
          <span>{rating}</span>
        </div>
        <div className="stat" title={`${plugin.stats.stars} stars`}>
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
            <path d="M12 2l3.09 6.26L22 9.27l-5 4.87 1.18 6.88L12 17.77l-6.18 3.25L7 14.14 2 9.27l6.91-1.01L12 2z" />
          </svg>
          <span>{formatDownloads(plugin.stats.stars)}</span>
        </div>
      </div>

      {/* Install Button */}
      <button
        className="install-button"
        onClick={handleInstallClick}
        aria-label={`Install ${plugin.name}`}
      >
        Install
      </button>

      {/* Featured Badge */}
      {featured && (
        <div className="featured-badge">
          Featured
        </div>
      )}

      <style>{`
        .plugin-card {
          position: relative;
          background: var(--bg-secondary);
          border: 1px solid var(--border-color);
          border-radius: 12px;
          padding: 20px;
          cursor: pointer;
          transition: all 0.2s ease;
        }

        .plugin-card:hover {
          border-color: var(--primary-color);
          box-shadow: 0 4px 12px rgba(0, 0, 0, 0.1);
          transform: translateY(-2px);
        }

        .plugin-card:focus {
          outline: none;
          border-color: var(--primary-color);
          box-shadow: 0 0 0 3px var(--primary-color-alpha);
        }

        .plugin-card.featured {
          border-color: var(--accent-color);
          background: linear-gradient(135deg, var(--bg-secondary) 0%, var(--bg-tertiary) 100%);
        }

        .card-header {
          display: flex;
          align-items: flex-start;
          gap: 12px;
          margin-bottom: 12px;
        }

        .plugin-icon {
          width: 48px;
          height: 48px;
          border-radius: 10px;
          overflow: hidden;
          flex-shrink: 0;
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
          font-size: 1.5rem;
          font-weight: 600;
        }

        .plugin-title {
          flex: 1;
          min-width: 0;
        }

        .plugin-title h3 {
          margin: 0;
          font-size: 1.1rem;
          font-weight: 600;
          display: flex;
          align-items: center;
          gap: 6px;
        }

        .verified-badge {
          color: var(--success-color);
          display: inline-flex;
        }

        .version {
          font-size: 0.85rem;
          color: var(--text-secondary);
        }

        .description {
          margin: 0 0 12px 0;
          font-size: 0.9rem;
          color: var(--text-secondary);
          line-height: 1.4;
          display: -webkit-box;
          -webkit-line-clamp: 2;
          -webkit-box-orient: vertical;
          overflow: hidden;
        }

        .author {
          font-size: 0.85rem;
          color: var(--text-secondary);
          margin-bottom: 12px;
          display: flex;
          align-items: center;
          gap: 4px;
        }

        .author-verified {
          color: var(--success-color);
        }

        .tags {
          display: flex;
          flex-wrap: wrap;
          gap: 6px;
          margin-bottom: 16px;
        }

        .category-tag {
          padding: 4px 8px;
          background: var(--primary-color-alpha);
          color: var(--primary-color);
          border-radius: 4px;
          font-size: 0.75rem;
          font-weight: 500;
        }

        .tag {
          padding: 4px 8px;
          background: var(--bg-tertiary);
          color: var(--text-secondary);
          border-radius: 4px;
          font-size: 0.75rem;
        }

        .stats-row {
          display: flex;
          gap: 16px;
          margin-bottom: 16px;
        }

        .stat {
          display: flex;
          align-items: center;
          gap: 4px;
          font-size: 0.85rem;
          color: var(--text-secondary);
        }

        .stat svg {
          width: 16px;
          height: 16px;
        }

        .install-button {
          width: 100%;
          padding: 10px;
          background: var(--primary-color);
          color: white;
          border: none;
          border-radius: 8px;
          font-weight: 600;
          cursor: pointer;
          transition: background 0.2s;
        }

        .install-button:hover {
          background: var(--primary-color-dark);
        }

        .featured-badge {
          position: absolute;
          top: 12px;
          right: 12px;
          padding: 4px 8px;
          background: var(--accent-color);
          color: white;
          border-radius: 4px;
          font-size: 0.75rem;
          font-weight: 600;
        }
      `}</style>
    </div>
  );
}