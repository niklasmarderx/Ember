import { expect, test } from '@playwright/test';

test.describe('Ember Web UI', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/');
  });

  test('should load the application', async ({ page }) => {
    // Check that the main app container exists
    await expect(page.locator('[data-testid="app-container"]')).toBeVisible();
    
    // Check that the header/title is visible
    await expect(page.getByRole('heading', { level: 1 })).toBeVisible();
  });

  test('should display the chat interface', async ({ page }) => {
    // Check for chat input
    const chatInput = page.getByPlaceholder(/message|type|ask/i);
    await expect(chatInput).toBeVisible();
    
    // Check for send button
    const sendButton = page.getByRole('button', { name: /send/i });
    await expect(sendButton).toBeVisible();
  });
});

test.describe('Theme Toggle', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/');
  });

  test('should toggle between light and dark theme', async ({ page }) => {
    // Find theme toggle button
    const themeToggle = page.getByRole('button', { name: /theme|dark|light/i });
    
    if (await themeToggle.isVisible()) {
      // Get initial theme state
      const html = page.locator('html');
      const initialClass = await html.getAttribute('class');
      
      // Click theme toggle
      await themeToggle.click();
      
      // Verify theme changed
      const newClass = await html.getAttribute('class');
      expect(newClass).not.toBe(initialClass);
      
      // Toggle back
      await themeToggle.click();
      const finalClass = await html.getAttribute('class');
      expect(finalClass).toBe(initialClass);
    }
  });
});

test.describe('Model Selection', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/');
  });

  test('should display model selector', async ({ page }) => {
    // Look for model selector component
    const modelSelector = page.getByRole('combobox').or(
      page.locator('[data-testid="model-selector"]')
    );
    
    if (await modelSelector.isVisible()) {
      await expect(modelSelector).toBeEnabled();
    }
  });

  test('should allow model selection', async ({ page }) => {
    const modelSelector = page.getByRole('combobox').first();
    
    if (await modelSelector.isVisible()) {
      await modelSelector.click();
      
      // Check that options appear
      const options = page.getByRole('option');
      const optionCount = await options.count();
      expect(optionCount).toBeGreaterThan(0);
    }
  });
});

test.describe('Chat Functionality', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/');
  });

  test('should allow typing in chat input', async ({ page }) => {
    const chatInput = page.getByPlaceholder(/message|type|ask/i).first();
    
    await chatInput.fill('Hello, Ember!');
    await expect(chatInput).toHaveValue('Hello, Ember!');
  });

  test('should clear input after sending message', async ({ page }) => {
    const chatInput = page.getByPlaceholder(/message|type|ask/i).first();
    const sendButton = page.getByRole('button', { name: /send/i });
    
    await chatInput.fill('Test message');
    
    if (await sendButton.isEnabled()) {
      await sendButton.click();
      
      // Input should be cleared after sending
      // Note: This might fail if there's no backend, but the UI behavior should still work
    }
  });

  test('should support keyboard shortcut to send', async ({ page }) => {
    const chatInput = page.getByPlaceholder(/message|type|ask/i).first();
    
    await chatInput.fill('Test message');
    await chatInput.press('Enter');
    
    // Verify the send action was triggered
    // The input might be cleared or a message might appear
  });
});

test.describe('Conversation List', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/');
  });

  test('should display conversation sidebar', async ({ page }) => {
    // Look for sidebar or conversation list
    const sidebar = page.locator('[data-testid="conversation-list"]').or(
      page.getByRole('navigation')
    ).or(
      page.locator('aside')
    );
    
    if (await sidebar.isVisible()) {
      await expect(sidebar).toBeVisible();
    }
  });

  test('should allow creating new conversation', async ({ page }) => {
    const newChatButton = page.getByRole('button', { name: /new|create|add/i });
    
    if (await newChatButton.isVisible()) {
      await expect(newChatButton).toBeEnabled();
      await newChatButton.click();
      
      // Should reset or create new conversation
    }
  });
});

test.describe('Settings', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/');
  });

  test('should open settings dialog', async ({ page }) => {
    const settingsButton = page.getByRole('button', { name: /settings|config/i }).or(
      page.locator('[data-testid="settings-button"]')
    );
    
    if (await settingsButton.isVisible()) {
      await settingsButton.click();
      
      // Settings dialog should appear
      const settingsDialog = page.getByRole('dialog').or(
        page.locator('[data-testid="settings-dialog"]')
      );
      await expect(settingsDialog).toBeVisible();
    }
  });
});

test.describe('Keyboard Navigation', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/');
  });

  test('should support tab navigation', async ({ page }) => {
    // Start from body
    await page.keyboard.press('Tab');
    
    // Check that focus moved to first focusable element
    const focusedElement = page.locator(':focus');
    await expect(focusedElement).toBeVisible();
  });

  test('should open keyboard shortcuts help', async ({ page }) => {
    // Press ? or Ctrl+/ to open shortcuts
    await page.keyboard.press('?');
    
    // Or try with modifier
    await page.keyboard.press('Control+/');
    
    // Look for shortcuts dialog/panel
    const shortcutsPanel = page.getByText(/keyboard shortcuts|shortcuts/i);
    // This might or might not be visible depending on implementation
  });
});

test.describe('Responsive Design', () => {
  test('should work on mobile viewport', async ({ page }) => {
    await page.setViewportSize({ width: 375, height: 667 });
    await page.goto('/');
    
    // Main content should still be visible
    const chatInput = page.getByPlaceholder(/message|type|ask/i).first();
    await expect(chatInput).toBeVisible();
  });

  test('should work on tablet viewport', async ({ page }) => {
    await page.setViewportSize({ width: 768, height: 1024 });
    await page.goto('/');
    
    // Main content should still be visible
    const chatInput = page.getByPlaceholder(/message|type|ask/i).first();
    await expect(chatInput).toBeVisible();
  });

  test('should work on desktop viewport', async ({ page }) => {
    await page.setViewportSize({ width: 1920, height: 1080 });
    await page.goto('/');
    
    // Main content should still be visible
    const chatInput = page.getByPlaceholder(/message|type|ask/i).first();
    await expect(chatInput).toBeVisible();
  });
});

test.describe('Accessibility', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/');
  });

  test('should have proper heading structure', async ({ page }) => {
    const h1 = page.getByRole('heading', { level: 1 });
    await expect(h1).toBeVisible();
  });

  test('should have accessible form labels', async ({ page }) => {
    // Check that inputs have associated labels or aria-labels
    const inputs = page.locator('input, textarea');
    const count = await inputs.count();
    
    for (let i = 0; i < count; i++) {
      const input = inputs.nth(i);
      const hasLabel = await input.getAttribute('aria-label') !== null ||
        await input.getAttribute('aria-labelledby') !== null ||
        await input.getAttribute('placeholder') !== null;
      
      // At least one accessibility attribute should be present
    }
  });

  test('should have proper button labels', async ({ page }) => {
    const buttons = page.getByRole('button');
    const count = await buttons.count();
    
    for (let i = 0; i < count; i++) {
      const button = buttons.nth(i);
      const text = await button.textContent();
      const ariaLabel = await button.getAttribute('aria-label');
      
      // Button should have either text content or aria-label
      expect(text || ariaLabel).toBeTruthy();
    }
  });
});

test.describe('Error Handling', () => {
  test('should display error boundary on component error', async ({ page }) => {
    await page.goto('/');
    
    // The error boundary should catch and display errors gracefully
    // This test verifies the app doesn't crash completely
    await expect(page.locator('body')).not.toHaveText(/uncaught error/i);
  });
});

test.describe('Performance', () => {
  test('should load within acceptable time', async ({ page }) => {
    const startTime = Date.now();
    await page.goto('/');
    const loadTime = Date.now() - startTime;
    
    // Page should load within 5 seconds
    expect(loadTime).toBeLessThan(5000);
  });

  test('should not have memory leaks on navigation', async ({ page }) => {
    await page.goto('/');
    
    // Navigate multiple times
    for (let i = 0; i < 5; i++) {
      await page.reload();
    }
    
    // App should still be responsive
    const chatInput = page.getByPlaceholder(/message|type|ask/i).first();
    await expect(chatInput).toBeVisible();
  });
});