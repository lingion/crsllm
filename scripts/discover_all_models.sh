#!/bin/bash
# 批量发现所有提供商的模型列表
# 用法: ./scripts/discover_all_models.sh [config_file]

CONFIG_FILE=${1:-"config/crsllm.toml"}

if [ ! -f "$CONFIG_FILE" ]; then
    echo "错误: 配置文件不存在: $CONFIG_FILE"
    echo "请创建配置文件，参考 config/crsllm.toml.example"
    exit 1
fi

echo "=== 开始发现模型列表 ==="
echo "配置文件: $CONFIG_FILE"
echo ""

# 运行发现命令
./target/release/crsllm --discover-models --config "$CONFIG_FILE"

echo ""
echo "=== 发现完成 ==="
echo "结果已保存到: ${CONFIG_FILE%.toml}.discovered.toml"
