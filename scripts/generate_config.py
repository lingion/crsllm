#!/usr/bin/env python3
"""
从 FreeLLMAPI 配置生成 CRSLLM 配置文件
自动提取所有 API Key 和提供商信息
"""

import json
import sys
from datetime import datetime

def load_freellmapi_config():
    """加载 FreeLLMAPI 配置"""
    with open('/Users/lingion_k/freellmapi-import/freellmapi.config.json', 'r') as f:
        return json.load(f)

def generate_crsllm_config():
    """生成 CRSLLM 配置"""
    config = load_freellmapi_config()
    custom_providers = config.get('customProviders', [])
    
    print(f"# CRSLLM 配置文件")
    print(f"# 自动生成时间: {datetime.now().isoformat()}")
    print(f"# 从 FreeLLMAPI 导入 {len(custom_providers)} 个自定义提供商")
    print()
    
    # 生成 server 配置
    print("[server]")
    print("host = \"127.0.0.1\"")
    print("port = 3001")
    print()
    
    # 生成 timeouts 配置
    print("[timeouts]")
    print("connect = 10")
    print("read = 60")
    print("total = 120")
    print()
    
    # 生成 retry 配置
    print("[retry]")
    print("max_retries = 3")
    print("backoff_factor = 2")
    print("max_backoff = 60")
    print()
    
    # 生成 cooldown 配置
    print("[cooldown]")
    print("default_duration = 300")
    print("max_duration = 3600")
    print()
    
    # 为每个自定义提供商生成 provider 配置
    for i, provider in enumerate(custom_providers):
        label = provider.get('label', f'provider-{i}')
        base_url = provider.get('baseUrl', '')
        api_key = provider.get('apiKey', '')
        models = provider.get('models', [])
        
        if not base_url or not api_key:
            continue
        
        print(f"# {label}")
        print(f"[providers.{label}]")
        print(f"base_url = \"{base_url}\"")
        print(f"api_key = \"{api_key}\"")
        print(f"auth_type = \"bearer\"")
        
        # 如果有模型列表，添加到配置
        if models:
            model_names = [m.get('model') for m in models if m.get('model')]
            if model_names:
                print(f"models = {json.dumps(model_names)}")
        
        print()
    
    # 生成示例模型组
    print("# === 模型组配置示例 ===")
    print("# 取消注释并根据需要修改")
    print()
    print("# [[model_groups]]")
    print("# name = \"claude-opus-5\"")
    print("# providers = [")
    print("#   { name = \"agentrouter\", model_id = \"claude-opus-5\" },")
    print("#   { name = \"tabitoken\", model_id = \"claude-opus-5\" },")
    print("#   { name = \"gorouter\", model_id = \"claude-opus-5\" },")
    print("# ]")
    print()
    print("# [[model_groups]]")
    print("# name = \"gpt-5.6-luna\"")
    print("# providers = [")
    print("#   { name = \"motomoto\", model_id = \"gpt-5.6-luna\" },")
    print("# ]")
    print()
    
    # 生成示例 fallback 链
    print("# === Fallback 链配置示例 ===")
    print("# 取消注释并根据需要修改")
    print()
    print("# [[fallback_chains]]")
    print("# name = \"default-chain\"")
    print("# models = [\"claude-opus-5\", \"gpt-5.6-luna\", \"deepseek-v4-pro\"]")
    print()
    
    # 输出统计信息
    print(f"# === 统计信息 ===")
    print(f"# 提供商总数: {len(custom_providers)}")
    print(f"# 模型总数: {sum(len(p.get('models', [])) for p in custom_providers)}")

if __name__ == '__main__':
    generate_crsllm_config()
