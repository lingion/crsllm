#!/usr/bin/env python3
"""
测试提供商连接性和模型发现
不需要 API Key，测试哪些提供商可以公开访问
"""

import requests
import json
import time
from concurrent.futures import ThreadPoolExecutor, as_completed

# 从 providers.toml 读取提供商列表
def load_providers():
    providers = {}
    with open('config/providers.toml', 'r') as f:
        content = f.read()
    
    current = None
    for line in content.split('\n'):
        line = line.strip()
        if line.startswith('[registry.'):
            name = line.split('.')[1].rstrip(']')
            providers[name] = {}
            current = name
        elif current and '=' in line:
            key, val = line.split('=', 1)
            key = key.strip()
            val = val.strip().strip('"')
            providers[current][key] = val
    
    return providers

def test_provider(name, config):
    """测试单个提供商的连接性"""
    base_url = config.get('base_url', '')
    models_path = config.get('models_path', '/v1/models')
    auth_type = config.get('auth_type', 'bearer')
    
    url = base_url.rstrip('/') + models_path
    
    try:
        # 尝试无认证访问
        headers = {}
        if auth_type == 'bearer':
            headers['Authorization'] = 'Bearer test'
        elif auth_type == 'api_key':
            headers['x-api-key'] = 'test'
        
        response = requests.get(url, headers=headers, timeout=10)
        
        if response.status_code == 200:
            data = response.json()
            models = []
            
            # 解析不同格式的响应
            if 'data' in data:
                models = [m.get('id') for m in data['data'] if m.get('id')]
            elif 'models' in data:
                models = [m.get('name', '').replace('models/', '') for m in data['models'] if m.get('name')]
            
            return {
                'name': name,
                'status': 'success',
                'models_count': len(models),
                'models': models[:10],  # 只显示前10个
                'url': url
            }
        elif response.status_code == 401 or response.status_code == 403:
            return {
                'name': name,
                'status': 'auth_required',
                'message': f'需要认证 (HTTP {response.status_code})',
                'url': url
            }
        else:
            return {
                'name': name,
                'status': 'error',
                'message': f'HTTP {response.status_code}',
                'url': url
            }
    except requests.exceptions.Timeout:
        return {
            'name': name,
            'status': 'timeout',
            'message': '连接超时',
            'url': url
        }
    except Exception as e:
        return {
            'name': name,
            'status': 'error',
            'message': str(e),
            'url': url
        }

def main():
    print("=== 提供商连接性测试 ===\n")
    
    providers = load_providers()
    print(f"测试 {len(providers)} 个提供商...\n")
    
    results = {
        'success': [],
        'auth_required': [],
        'timeout': [],
        'error': []
    }
    
    # 并行测试
    with ThreadPoolExecutor(max_workers=10) as executor:
        futures = {executor.submit(test_provider, name, config): name 
                   for name, config in providers.items()}
        
        for future in as_completed(futures):
            result = future.result()
            status = result['status']
            results[status].append(result)
            
            # 实时输出
            if status == 'success':
                print(f"✓ {result['name']:30s} - {result['models_count']} 个模型")
            elif status == 'auth_required':
                print(f"⚠ {result['name']:30s} - 需要认证")
            elif status == 'timeout':
                print(f"✗ {result['name']:30s} - 超时")
            else:
                print(f"✗ {result['name']:30s} - {result['message']}")
    
    # 汇总
    print("\n=== 测试结果汇总 ===")
    print(f"✓ 可访问: {len(results['success'])} 个")
    print(f"⚠ 需要认证: {len(results['auth_required'])} 个")
    print(f"✗ 超时: {len(results['timeout'])} 个")
    print(f"✗ 错误: {len(results['error'])} 个")
    
    if results['success']:
        print("\n=== 可访问的提供商 ===")
        for r in results['success']:
            print(f"\n{r['name']} ({r['models_count']} 个模型):")
            for model in r['models']:
                print(f"  - {model}")
            if r['models_count'] > 10:
                print(f"  ... 还有 {r['models_count'] - 10} 个模型")

if __name__ == '__main__':
    main()
