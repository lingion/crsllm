# CRSLLM 提供商和模型清单

> 生成时间: 2026-01-XX
> 测试方法: 实际连接测试（不需要 API Key 的提供商）

---

## 📊 总体统计

- **提供商总数**: 81 个
- **认证类型分布**:
  - Bearer Token: 79 个
  - API Key: 1 个 (google)
  - Query Parameter: 1 个 (google)

---

## ✅ 可访问的提供商（无需认证）

### 1. Ollama (本地)
- **Base URL**: http://localhost:11434
- **模型数量**: 4 个
- **模型列表**:
  - fable38-sft:latest
  - claude-heretic-thinking:latest
  - qwopus-coder-fable5:latest
  - qwen3-vl:4b-instruct

### 2. OpenRouter
- **Base URL**: https://openrouter.ai/api
- **模型数量**: 465 个
- **部分模型列表** (前 10 个):
  - moondream/moondream3-preview
  - moondream/moondream3.1-9b-a2b
  - meta/muse-spark-open
  - nvidia/nemotron-3.5-super-vl
  - nvidia/nemotron-3.5-super-vl:free
  - ~unbiased/pareto-latest
  - unbiased/pareto-26.9
  - liquid/lfm-2.5-24b-a2b
  - google/gemini-4-argon
  - google/gemini-4-argon:batch
  - ... 还有 455 个模型

---

## 🔐 需要认证的提供商（14 个）

以下提供商需要有效的 API Key 才能访问：

1. **openai-10** - https://api.openai.com/v1
2. **anthropic** - https://api.anthropic.com
3. **deepseek** - https://api.deepseek.com/v1
4. **deepseek-chat** - https://api.deepseek.com/v1
5. **deepseek-coder** - https://api.deepseek.com/v1
6. **groq** - https://api.groq.com/openai/v1
7. **openai-27** - https://api.openai.com/v1
8. **together** - https://api.together.xyz/v1
9. **replicate** - https://api.replicate.com/v1
10. **volcengine** - https://ark.cn-beijing.volces.com/api/v3
11. **deepinfra** - https://api.deepinfra.com/v1/openai
12. **cohere** - https://api.cohere.ai/v1
13. **siliconflow** - https://api.siliconflow.cn/v1
14. **minimax** - https://api.minimax.chat/v1

---

## ❌ 无法访问的提供商（55 个）

### DNS 解析失败（域名不存在或无法解析）
- openai-base, openai-extra, openai-11, openai-9, openai-12, openai-18, openai-20, openai-me, openai-cloud, novamax, bigone, tencent, lingyi, xinghuo, 360, lambdalabs, yandex, sagemaker, databricks, azure

### SSL/TLS 错误
- openai-19 (自签名证书)
- openai-24 (主机名不匹配)
- openai-tech (自签名证书)
- openai-26 (TLS 内部错误)
- baichuan (主机名不匹配)
- togethercomputer (TLS 未识别名称)

### 连接被拒绝
- openai-16, openai-dev, lmstudio, vllm, triton

### 连接超时
- openai-8, openai-23, openai-run, openai, openai-student, huggingface, lepton, poe, perplexity, mistral

### HTTP 错误
- openai-25 (404)
- openai-15 (404)
- anyscale (404)
- fireworks (404)
- bedrock (404)
- anthropic-bedrock (404)
- aliyun (404)
- bytedance (404)
- targon (410)
- tongyi (404)
- runpod (404)

### 其他错误
- openai-13, openai-14, openai-17, openai-22, openai-io, openai-ai, openai-app (JSON 解析错误)
- openai-site, openai-21 (连接中断)
- google (URL 格式错误: generativelanguage.googleapis.comv1)
- tencent-hunyuan, baidu (JSON 解析错误)

---

## 📝 说明

### 为什么很多提供商无法访问？

1. **需要真实 API Key**: 大部分提供商需要有效的认证才能访问模型列表
2. **域名已失效**: 一些第三方 OpenAI 兼容服务的域名已经不存在
3. **网络问题**: 某些服务可能在中国大陆无法直接访问
4. **本地服务未启动**: lmstudio, vllm, triton, ollama 需要本地运行

### 如何获取完整模型列表？

1. **配置真实 API Key**:
   ```bash
   cp config/crsllm.toml.example config/crsllm.toml
   # 编辑 crsllm.toml，填入你的 API Key
   ```

2. **运行模型发现**:
   ```bash
   ./scripts/discover_all_models.sh config/crsllm.toml
   ```

3. **查看结果**:
   ```bash
   cat config/crsllm.discovered.toml
   ```

### 已验证可用的提供商

目前无需任何配置即可使用的提供商：
- **Ollama** (本地，需要先启动 Ollama 服务)
- **OpenRouter** (需要 API Key，但可以访问模型列表)

---

## 🎯 下一步

1. 用户需要配置真实的 API Key 到 `config/crsllm.toml`
2. 运行 `./scripts/discover_all_models.sh` 获取所有可用模型
3. 系统会自动保存发现的模型到 `.discovered.toml`
4. 配置模型组和 fallback 链
5. 启动服务开始使用

---

## 📚 相关文档

- [架构设计](../ARCHITECTURE.md)
- [配置示例](../config/crsllm.toml.example)
- [提供商注册表](../config/providers.toml)
