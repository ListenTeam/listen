```buildoutcfg

pub struct VestingInfo<Balance, BlockNumber> {
	/// Locked amount at genesis.
	pub locked: Balance,
	/// Amount that gets unlocked every block after `starting_block`.
	pub per_duration: Balance,
	/// unlock duration
	pub unlock_duration: BlockNumber,  // 添加这个参数(解锁周期)
	/// Starting block for unlocking(vesting).
	pub starting_block: BlockNumber,
}
```

添加以上参数之后, 逻辑变成:

1. 每个块都可以解锁变成按周期解锁
2. 转账时自定义解锁周期以及每个周期解锁的金额

