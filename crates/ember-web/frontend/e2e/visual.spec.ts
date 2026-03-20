import { expect, test } from '@playwright/test';

/**
 * Visual Regression Tests for Ember Web UI
 * 
 * These tests capture screenshots for visual comparison to detect
 * unintended UI changes between releases.
 */

// Configure visual comparison settings
const visualConfig = {
  threshold: 0.2, // Pixel difference threshold (0-1)
  maxDiffPixels: 100, // Maximum allowed different pixels
  animations: 'disabled' as const,
};

test.describe('Visual Regression - Core Components', () => {
  test.beforeEach(async ({ page }) => {
    // Disable animations for consistent screenshots
    await page.emulateMedia({ reducedMotion: 'reduce' });
  });

  test('homepage - light theme', async ({ page }) => {
    await page.goto('/');
    await page.waitForLoadState('networkidle');
    
    // Ensure light theme
    await page.evaluate(() => {
      document.documentElement.classList.remove('dark');
      localStorage.setItem('theme', 'light');
    });
    
    await page.waitForTimeout(500); // Wait for theme transition
    
    await expect(page).toHaveScreenshot('homepage-light.png', {
      fullPage: true,
      ...visualConfig,
    });
  });

  test('homepage - dark theme', async ({ page }) => {
    await page.goto('/');
    await page.waitForLoadState('networkidle');
    
    // Enable dark theme
    await page.evaluate(() => {
      document.documentElement.classList.add('dark');
      localStorage.setItem('theme', 'dark');
    });
    
    await page.waitForTimeout(500);
    
    await expect(page).toHaveScreenshot('homepage-dark.png', {
      fullPage: true,
      ...visualConfig,
    });
  });

  test('chat interface - empty state', async ({ page }) => {
    await page.goto('/');
    await page.waitForLoadState('networkidle');
    
    const chatArea = page.locator('[data-testid="chat-area"]');
    if (await chatArea.count() > 0) {
      await expect(chatArea).toHaveScreenshot('chat-empty.png', visualConfig);
    }
  });

  test('chat interface - with messages', async ({ page }) => {
    await page.goto('/');
    await page.waitForLoadState('networkidle');
    
    // Add test messages via API or UI interaction
    const input = page.locator('[data-testid="chat-input"]');
    if (await input.count() > 0) {
      await input.fill('Hello, this is a test message');
      await page.keyboard.press('Enter');
      
      // Wait for response
      await page.waitForSelector('[data-testid="assistant-message"]', {
        timeout: 30000,
      }).catch(() => {});
      
      const chatArea = page.locator('[data-testid="chat-area"]');
      await expect(chatArea).toHaveScreenshot('chat-with-messages.png', visualConfig);
    }
  });
});

test.describe('Visual Regression - UI Components', () => {
  test('model selector dropdown', async ({ page }) => {
    await page.goto('/');
    await page.waitForLoadState('networkidle');
    
    const modelSelector = page.locator('[data-testid="model-selector"]');
    if (await modelSelector.count() > 0) {
      await modelSelector.click();
      await page.waitForTimeout(300);
      
      await expect(page.locator('[data-testid="model-dropdown"]')).toHaveScreenshot(
        'model-selector-open.png',
        visualConfig
      );
    }
  });

  test('settings panel', async ({ page }) => {
    await page.goto('/');
    await page.waitForLoadState('networkidle');
    
    const settingsButton = page.locator('[data-testid="settings-button"]');
    if (await settingsButton.count() > 0) {
      await settingsButton.click();
      await page.waitForTimeout(300);
      
      const settingsPanel = page.locator('[data-testid="settings-panel"]');
      await expect(settingsPanel).toHaveScreenshot('settings-panel.png', visualConfig);
    }
  });

  test('conversation list', async ({ page }) => {
    await page.goto('/');
    await page.waitForLoadState('networkidle');
    
    const sidebar = page.locator('[data-testid="conversation-sidebar"]');
    if (await sidebar.count() > 0) {
      await expect(sidebar).toHaveScreenshot('conversation-sidebar.png', visualConfig);
    }
  });

  test('cost dashboard', async ({ page }) => {
    await page.goto('/');
    await page.waitForLoadState('networkidle');
    
    const dashboard = page.locator('[data-testid="cost-dashboard"]');
    if (await dashboard.count() > 0) {
      await expect(dashboard).toHaveScreenshot('cost-dashboard.png', visualConfig);
    }
  });
});

test.describe('Visual Regression - Plugin Marketplace', () => {
  test('marketplace overview', async ({ page }) => {
    await page.goto('/');
    await page.waitForLoadState('networkidle');
    
    // Navigate to marketplace if available
    const marketplaceTab = page.locator('[data-testid="marketplace-tab"]');
    if (await marketplaceTab.count() > 0) {
      await marketplaceTab.click();
      await page.waitForTimeout(500);
      
      await expect(page).toHaveScreenshot('marketplace-overview.png', {
        fullPage: true,
        ...visualConfig,
      });
    }
  });

  test('plugin card', async ({ page }) => {
    await page.goto('/');
    await page.waitForLoadState('networkidle');
    
    const pluginCard = page.locator('[data-testid="plugin-card"]').first();
    if (await pluginCard.count() > 0) {
      await expect(pluginCard).toHaveScreenshot('plugin-card.png', visualConfig);
    }
  });

  test('plugin details modal', async ({ page }) => {
    await page.goto('/');
    await page.waitForLoadState('networkidle');
    
    const pluginCard = page.locator('[data-testid="plugin-card"]').first();
    if (await pluginCard.count() > 0) {
      await pluginCard.click();
      await page.waitForTimeout(300);
      
      const detailsModal = page.locator('[data-testid="plugin-details"]');
      if (await detailsModal.count() > 0) {
        await expect(detailsModal).toHaveScreenshot('plugin-details.png', visualConfig);
      }
    }
  });
});

test.describe('Visual Regression - Responsive Design', () => {
  const viewports = [
    { name: 'mobile', width: 375, height: 667 },
    { name: 'tablet', width: 768, height: 1024 },
    { name: 'desktop', width: 1280, height: 800 },
    { name: 'wide', width: 1920, height: 1080 },
  ];

  for (const viewport of viewports) {
    test(`homepage - ${viewport.name} viewport`, async ({ page }) => {
      await page.setViewportSize({ width: viewport.width, height: viewport.height });
      await page.goto('/');
      await page.waitForLoadState('networkidle');
      
      await expect(page).toHaveScreenshot(`homepage-${viewport.name}.png`, {
        fullPage: true,
        ...visualConfig,
      });
    });
  }
});

test.describe('Visual Regression - States & Interactions', () => {
  test('loading state', async ({ page }) => {
    await page.goto('/');
    
    // Capture loading state before content loads
    await expect(page).toHaveScreenshot('loading-state.png', {
      ...visualConfig,
    });
  });

  test('error state', async ({ page }) => {
    // Simulate error by going to invalid route
    await page.goto('/invalid-route-for-testing');
    await page.waitForLoadState('networkidle');
    
    await expect(page).toHaveScreenshot('error-404.png', {
      fullPage: true,
      ...visualConfig,
    });
  });

  test('button hover states', async ({ page }) => {
    await page.goto('/');
    await page.waitForLoadState('networkidle');
    
    const primaryButton = page.locator('button.primary, [data-testid="primary-button"]').first();
    if (await primaryButton.count() > 0) {
      await primaryButton.hover();
      await expect(primaryButton).toHaveScreenshot('button-hover.png', visualConfig);
    }
  });

  test('input focus state', async ({ page }) => {
    await page.goto('/');
    await page.waitForLoadState('networkidle');
    
    const input = page.locator('[data-testid="chat-input"]');
    if (await input.count() > 0) {
      await input.focus();
      await expect(input).toHaveScreenshot('input-focused.png', visualConfig);
    }
  });
});

test.describe('Visual Regression - Tool Execution', () => {
  test('tool execution indicator', async ({ page }) => {
    await page.goto('/');
    await page.waitForLoadState('networkidle');
    
    // This would typically need a mock or actual tool execution
    const toolIndicator = page.locator('[data-testid="tool-execution"]');
    if (await toolIndicator.count() > 0) {
      await expect(toolIndicator).toHaveScreenshot('tool-execution.png', visualConfig);
    }
  });

  test('tool result display', async ({ page }) => {
    await page.goto('/');
    await page.waitForLoadState('networkidle');
    
    const toolResult = page.locator('[data-testid="tool-result"]').first();
    if (await toolResult.count() > 0) {
      await expect(toolResult).toHaveScreenshot('tool-result.png', visualConfig);
    }
  });
});

test.describe('Visual Regression - Accessibility', () => {
  test('high contrast mode', async ({ page }) => {
    await page.goto('/');
    await page.waitForLoadState('networkidle');
    
    // Enable high contrast mode
    await page.emulateMedia({ colorScheme: 'dark', forcedColors: 'active' });
    
    await expect(page).toHaveScreenshot('high-contrast.png', {
      fullPage: true,
      ...visualConfig,
    });
  });

  test('reduced motion', async ({ page }) => {
    await page.goto('/');
    await page.emulateMedia({ reducedMotion: 'reduce' });
    await page.waitForLoadState('networkidle');
    
    // Trigger an animation (e.g., open dropdown)
    const dropdown = page.locator('[data-testid="model-selector"]');
    if (await dropdown.count() > 0) {
      await dropdown.click();
      await page.waitForTimeout(100);
      
      await expect(page).toHaveScreenshot('reduced-motion.png', visualConfig);
    }
  });
});

test.describe('Visual Regression - RTL Support', () => {
  test('RTL layout - Arabic', async ({ page }) => {
    await page.goto('/');
    await page.waitForLoadState('networkidle');
    
    // Set RTL direction
    await page.evaluate(() => {
      document.documentElement.dir = 'rtl';
      document.documentElement.lang = 'ar';
    });
    
    await page.waitForTimeout(300);
    
    await expect(page).toHaveScreenshot('rtl-layout.png', {
      fullPage: true,
      ...visualConfig,
    });
  });
});

// Utility function to update baseline screenshots
test.describe.skip('Update Baselines', () => {
  test('update all baselines', async ({ page }) => {
    // This test is skipped by default
    // Run with --update-snapshots flag to update baselines
    console.log('Run with --update-snapshots to update baseline screenshots');
  });
});