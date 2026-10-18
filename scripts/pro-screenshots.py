#!/usr/bin/env python3
"""Take professional, focused screenshots of crsllm dashboard"""
from playwright.sync_api import sync_playwright
import time

def take_focused_screenshots():
    with sync_playwright() as p:
        browser = p.chromium.launch(headless=True)
        page = browser.new_page(viewport={"width": 1400, "height": 900})
        
        # Navigate to dashboard
        page.goto("http://localhost:3002/dashboard")
        time.sleep(2)  # Wait for data to load
        
        # Screenshot 1: Model Groups - focused on the table
        print("Capturing model groups...")
        page.click('[data-tab="groups"]')
        time.sleep(1)
        # Find the table container and screenshot just that area
        table = page.locator('#groups-container').first
        if table.is_visible():
            table.screenshot(path='docs/screenshots/console-groups.png')
            print("  ✓ console-groups.png")
        
        # Screenshot 2: Routed Models - focused on the table
        print("Capturing routed models...")
        page.click('[data-tab="routed"]')
        time.sleep(1)
        table = page.locator('#routed-container').first
        if table.is_visible():
            table.screenshot(path='docs/screenshots/console-routed.png')
            print("  ✓ console-routed.png")
        
        # Screenshot 3: API Keys - focused on the table
        print("Capturing API keys...")
        page.click('[data-tab="keys"]')
        time.sleep(1)
        table = page.locator('#api-keys-container').first
        if table.is_visible():
            table.screenshot(path='docs/screenshots/console-keys.png')
            print("  ✓ console-keys.png")
        
        # Screenshot 4: Logs - focused on the table
        print("Capturing logs...")
        page.click('[data-tab="logs"]')
        time.sleep(1)
        table = page.locator('#panel-logs').first
        if table.is_visible():
            table.screenshot(path='docs/screenshots/console-logs.png')
            print("  ✓ console-logs.png")
        
        # Screenshot 5: Providers - focused on the cards
        print("Capturing providers...")
        page.click('[data-tab="providers"]')
        time.sleep(1)
        cards = page.locator('#providers-container').first
        if cards.is_visible():
            cards.screenshot(path='docs/screenshots/console-providers.png')
            print("  ✓ console-providers.png")
        
        # Screenshot 6: Fallback chain - capture the stats/metrics area
        print("Capturing metrics...")
        page.click('[data-tab="fallback"]')
        time.sleep(1)
        stats = page.locator('#panel-fallback').first
        if stats.is_visible():
            stats.screenshot(path='docs/screenshots/console-fallback.png')
            print("  ✓ console-fallback.png")
        
        browser.close()
        print("\nAll screenshots captured!")

if __name__ == "__main__":
    take_focused_screenshots()
