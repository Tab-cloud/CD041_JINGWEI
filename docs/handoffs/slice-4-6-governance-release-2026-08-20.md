# Slice 4/6：供应链、库治理与成熟度发布交接

> 日期：2026-08-20  
> 状态：Pass（本地确定性闭环）；实时商业数据、生产 IAM 与组织签名属于外部门禁

## 交付范围

- `jingwei-library` 提供 provider snapshot、TTL、许可证、缓存/数据分类、离线冻结、作用域审批/撤销、替代料
  复核、关键器件 3D 门禁和精确资产哈希影响索引。
- 正式 daemon 提供 `part_search`、`part_compare`、`part_select`、`part_substitute_validate`、
  `library_resolve`、`library_verify`、`library_governance_evaluate` 和 `schematic_create`。
- Rust 拥有中央 JDP tool catalog，生成 TypeScript catalog 并由 Runtime 注册；不可执行的领域名称不冒充能力。
- `jingwei-jir` 从 G0-G7 计算 L0-L4 assessment，按 revision、intent、toolchain、rules、manufacturer profile、
  evidence 与 waiver 生成失效原因。
- `jingwei-store` 持久化不可变 assessment、attestation、release request、signed release 和 revocation。
- Release request 必须绑定当前 L4 assessment 和完整、未过期的角色 attestation；独立 Ed25519 verifier 按组织
  公钥离线验证 manifest、角色、依赖、签名与撤销。签名覆盖 release ID、完整 request、signer key ID 和签发时间，
  防止通过改写 release ID 绕开撤销关联。

## 安全边界

- Agent 不能批准、拒绝或撤销自己的库资产；组织级推广生成资产需要组织管理员和明确人工证据。
- 过期、撤销、拒绝、许可证不允许或内容哈希变化均阻断绑定；离线冻结只代表可复现，不代表实时库存/价格。
- L5 不是 assessment 计算结果，只能来自独立组织 signer。当前 daemon 没有可信生产 IAM role claims，因此没有
  暴露让本地客户端自报角色、伪造签核的 API。
- UI 继续 fail closed 显示发布阻断；在可信 gate provider/IAM adapter 接入前不把本地测试升级为生产审批。

## 验证证据

- library governance 单元测试覆盖陈旧/冻结/撤销/无许可证/缺 3D/影响分析/旧哈希路径。
- daemon 集成测试通过真实 JDP 路径证明陈旧 snapshot 被拒绝。
- lifecycle engine 测试覆盖依赖失效、角色缺失、独立签名和已撤销 release。
- store 测试覆盖 assessment 持久化、碰撞和篡改拒绝。
- 生成 catalog 测试确认 `library_governance_evaluate` 与 `pcb_route_critical` 已进入 Runtime 可消费目录。

## 明确未替代的外部证据

- 已授权的实时商业器件/价格/库存 provider；
- 生产 OS/IAM identity adapter 和组织角色声明；
- 组织 release signer、代码签名证书与签名 updater；
- 制造商 DFM、实物样板、功能/热/可靠性测试和具名工程审批。

商业 OpenAI live test 因环境未配置 `OPENAI_API_KEY` 与 `JINGWEI_OPENAI_MODEL` 明确跳过，不计为通过。
