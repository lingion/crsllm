#!/usr/bin/env python3
"""Inject mock data into crsllm for professional screenshots"""
import requests
import json

BASE = "http://localhost:3002"

# Create model groups with realistic names
model_groups = [
    {
        "name": "gpt-4-family",
        "providers": [
            {"provider": "openai", "model": "gpt-4-turbo-preview"},
            {"provider": "openai", "model": "gpt-4"},
            {"provider": "openai", "model": "gpt-4o"}
        ]
    },
    {
        "name": "claude-family",
        "providers": [
            {"provider": "anthropic", "model": "claude-3-opus-20240229"},
            {"provider": "anthropic", "model": "claude-3-sonnet-20240229"},
            {"provider": "anthropic", "model": "claude-3-haiku-20240307"}
        ]
    },
    {
        "name": "fast-models",
        "providers": [
            {"provider": "openai", "model": "gpt-3.5-turbo"},
            {"provider": "anthropic", "model": "claude-3-haiku-20240307"},
            {"provider": "google", "model": "gemini-pro"}
        ]
    }
]

# Create routed models (client-facing aliases)
routed_models = [
    {
        "name": "smart",
        "chain": ["claude-family", "gpt-4-family"]
    },
    {
        "name": "fast",
        "chain": ["fast-models"]
    },
    {
        "name": "balanced",
        "chain": ["gpt-4-family", "claude-family"]
    }
]

# Create API keys
api_keys = [
    {"name": "production-app", "description": "Main production application"},
    {"name": "staging-env", "description": "Staging environment"},
    {"name": "dev-team", "description": "Development team access"}
]

print("Creating model groups...")
for g in model_groups:
    try:
        r = requests.post(f"{BASE}/api/model-groups", json=g)
        if r.status_code == 201:
            print(f"  ✓ {g['name']}")
        else:
            print(f"  ✗ {g['name']}: {r.text}")
    except Exception as e:
        print(f"  ✗ {g['name']}: {e}")

print("\nCreating routed models...")
for r_model in routed_models:
    try:
        r = requests.post(f"{BASE}/api/routed-models", json=r_model)
        if r.status_code == 201:
            print(f"  ✓ {r_model['name']}")
        else:
            print(f"  ✗ {r_model['name']}: {r.text}")
    except Exception as e:
        print(f"  ✗ {r_model['name']}: {e}")

print("\nCreating API keys...")
for k in api_keys:
    try:
        r = requests.post(f"{BASE}/api/api-keys", json=k)
        if r.status_code == 201:
            data = r.json()
            print(f"  ✓ {k['name']}: {data.get('key', 'created')}")
        else:
            print(f"  ✗ {k['name']}: {r.text}")
    except Exception as e:
        print(f"  ✗ {k['name']}: {e}")

print("\nSending test requests to generate logs...")
# Send some test requests to generate log entries
test_requests = [
    {"model": "smart", "messages": [{"role": "user", "content": "Hello"}]},
    {"model": "fast", "messages": [{"role": "user", "content": "Hi"}]},
    {"model": "balanced", "messages": [{"role": "user", "content": "Test"}]},
]

for i, req in enumerate(test_requests):
    try:
        r = requests.post(f"{BASE}/v1/chat/completions", json=req)
        print(f"  Request {i+1}: {r.status_code}")
    except Exception as e:
        print(f"  Request {i+1}: {e}")

print("\nMock data injection complete!")
